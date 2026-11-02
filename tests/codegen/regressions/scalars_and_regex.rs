//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of regressions scalars and regex, including null byte in string, not empty string is true, and not nonempty string is false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests that `strlen()` correctly counts bytes including embedded null characters.
/// Fixture: a string with an embedded null byte (`"ab\0cd"` has 5 bytes).
#[test]
fn test_null_byte_in_string() {
    let out = compile_and_run(r#"<?php echo strlen("ab\0cd");"#);
    assert_eq!(out, "5");
}

// -- Issue #26: empty string should be falsy --

/// Verifies that an empty string is falsy (double negation yields empty string = false).
/// Regression for issue #26.
#[test]
fn test_not_empty_string_is_true() {
    let out = compile_and_run(r#"<?php echo !!"";"#);
    assert_eq!(out, "");
}

/// Verifies that a non-empty string is truthy (double negation yields "1").
/// Regression for issue #26.
#[test]
fn test_not_nonempty_string_is_false() {
    let out = compile_and_run(r#"<?php echo !!"hello";"#);
    assert_eq!(out, "1");
}

// -- Issue #27: is_numeric() should work for numeric strings --

/// Verifies that `is_numeric()` returns true for decimal digit strings.
/// Regression for issue #27.
#[test]
fn test_is_numeric_string_digits() {
    let out = compile_and_run(r#"<?php if (is_numeric("42")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

/// Verifies that `is_numeric()` returns true for floating-point strings.
/// Regression for issue #27.
#[test]
fn test_is_numeric_string_float() {
    let out =
        compile_and_run(r#"<?php if (is_numeric("3.14")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

/// Verifies that `is_numeric()` returns true for negative numeric strings.
/// Regression for issue #27.
#[test]
fn test_is_numeric_string_negative() {
    let out = compile_and_run(r#"<?php if (is_numeric("-5")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

/// Verifies that `is_numeric()` returns false for non-numeric strings.
/// Regression for issue #27.
#[test]
fn test_is_numeric_string_not_numeric() {
    let out =
        compile_and_run(r#"<?php if (is_numeric("abc")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "no");
}

// -- Issue #29: function_exists() should recognize builtins --

/// Verifies that `preg_split()` correctly handles the `\s+` regex pattern (whitespace splitting).
/// Regression for issue #29 (function_exists() recognizing builtins).
#[test]
fn test_preg_split_backslash_s() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/\s+/", "hello  world");
echo $parts[1];
"#,
    );
    assert_eq!(out, "world");
}

/// Verifies that `preg_split()` correctly handles the `\d+` regex pattern (digit splitting).
/// Regression for issue #29 (function_exists() recognizing builtins).
#[test]
fn test_preg_split_backslash_d() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/\d+/", "abc123def456ghi");
echo count($parts) . "|" . $parts[0] . "|" . $parts[1] . "|" . $parts[2];
"#,
    );
    assert_eq!(out, "3|abc|def|ghi");
}

/// Verifies that `preg_match()` correctly handles the `\s` regex pattern (single whitespace).
/// Regression for issue #29 (function_exists() recognizing builtins).
#[test]
fn test_preg_match_backslash_s() {
    let out = compile_and_run(r#"<?php echo preg_match("/\s/", "hello world");"#);
    assert_eq!(out, "1");
}

/// Verifies that `preg_match()` correctly handles the `\d+` regex pattern (digits).
/// Regression for issue #29 (function_exists() recognizing builtins).
#[test]
fn test_preg_match_backslash_d() {
    let out = compile_and_run(r#"<?php echo preg_match("/\d+/", "abc123");"#);
    assert_eq!(out, "1");
}

/// Verifies that `preg_match()` correctly handles the `\w+` regex pattern (word characters).
/// Regression for issue #29 (function_exists() recognizing builtins).
#[test]
fn test_preg_match_backslash_w() {
    let out = compile_and_run(r#"<?php echo preg_match("/^\w+$/", "hello_world");"#);
    assert_eq!(out, "1");
}

// --- Issue #14: hex integer literals ---

/// Verifies that lowercase hex literal `0xff` is parsed and evaluates to 255.
/// Regression for issue #14 (hex integer literals).
#[test]
fn test_hex_literal_0xff() {
    let out = compile_and_run("<?php echo 0xFF;");
    assert_eq!(out, "255");
}

/// Verifies that mixed-case hex literal `0x1a` is parsed case-insensitively and evaluates to 26.
/// Regression for issue #14 (hex integer literals).
#[test]
fn test_hex_literal_0x1a() {
    let out = compile_and_run("<?php echo 0x1A;");
    assert_eq!(out, "26");
}

/// Verifies that zero hex literal `0x0` is parsed and evaluates to 0.
/// Regression for issue #14 (hex integer literals).
#[test]
fn test_hex_literal_0x0() {
    let out = compile_and_run("<?php echo 0x0;");
    assert_eq!(out, "0");
}

/// Verifies that the `0X` prefix (uppercase X) is also accepted for hex literals.
/// Regression for issue #14 (hex integer literals).
#[test]
fn test_hex_literal_uppercase_prefix() {
    let out = compile_and_run("<?php echo 0XFF;");
    assert_eq!(out, "255");
}

/// Verifies that hex literals can be used in arithmetic expressions.
/// Regression for issue #14 (hex integer literals).
#[test]
fn test_hex_literal_arithmetic() {
    let out = compile_and_run("<?php echo 0xFF + 1;");
    assert_eq!(out, "256");
}

// --- Issue #23: modulo by zero ---

/// Verifies normal modulo behavior: `5 % 1` returns 0 (no remainder).
/// Regression for issue #23 (modulo by zero).
#[test]
fn test_modulo_normal() {
    let out = compile_and_run("<?php echo 5 % 1;");
    assert_eq!(out, "0");
}

/// Verifies that an uncaught modulo by zero is a `DivisionByZeroError` fatal, not the value `0`.
///
/// Regression for issue #23 (modulo by zero). Reference PHP 8.4 raises
/// `DivisionByZeroError: Modulo by zero`; elephc used to fall back to `mov result, 0` and hand
/// back `0`. With no handler active the program still terminates — the diagnostic just names
/// the PHP class now, matching `Fatal error: Uncaught DivisionByZeroError: Modulo by zero`.
#[test]
fn test_modulo_by_zero_is_uncaught_fatal() {
    let err = compile_and_run_expect_failure("<?php echo 5 % 0;");
    assert!(
        err.contains("Uncaught DivisionByZeroError: Modulo by zero"),
        "modulo by zero should fatal as a DivisionByZeroError, got: {err}"
    );
}

/// Verifies the modulo-by-zero error is catchable, which is the substantive half of PHP 8's
/// behavior: the `catch` block runs and `getMessage()` carries php-src's own wording.
///
/// Regression for issue #23 (modulo by zero). While `%` produced a value, no `catch` clause
/// could ever observe a zero divisor.
#[test]
fn test_modulo_by_zero_is_catchable() {
    let out = compile_and_run(
        "<?php try { echo 5 % 0; } catch (DivisionByZeroError $e) { echo get_class($e), ':', $e->getMessage(); }",
    );
    assert_eq!(out, "DivisionByZeroError:Modulo by zero");
}

/// Verifies normal modulo remainder: `7 % 3` returns 1.
/// Regression for issue #23 (modulo by zero).
#[test]
fn test_modulo_normal_remainder() {
    let out = compile_and_run("<?php echo 7 % 3;");
    assert_eq!(out, "1");
}

// --- Issue #24: negative array index ---

/// Verifies that `fmod(-10, 3)` returns `-1` (negative dividend modulo).
/// Regression for issue #24 (negative array index / float modulo).
#[test]
fn test_fmod_negative_dividend() {
    let out = compile_and_run("<?php echo fmod(-10, 3);");
    assert_eq!(out, "-1");
}

/// Verifies that float modulo with a negative dividend returns a negative remainder.
/// Regression for issue #24 (negative array index / float modulo).
#[test]
fn test_float_modulo_negative() {
    let out = compile_and_run("<?php echo -10.0 % 3;");
    assert_eq!(out, "-1");
}

// --- Bug fix: string "0" is falsy ---

/// Verifies that the string `"0"` is falsy in an if statement.
/// Bug fix: string "0" is falsy.
#[test]
fn test_string_zero_falsy_if() {
    let out = compile_and_run(
        r#"<?php
if ("0") { echo "bad"; } else { echo "good"; }
"#,
    );
    assert_eq!(out, "good");
}

/// Verifies that the string `"0"` is falsy in a ternary expression.
/// Bug fix: string "0" is falsy.
#[test]
fn test_string_zero_falsy_ternary() {
    let out = compile_and_run(r#"<?php echo "0" ? "truthy" : "falsy";"#);
    assert_eq!(out, "falsy");
}

/// Verifies that negation of the string `"0"` is truthy.
/// Bug fix: string "0" is falsy.
#[test]
fn test_string_zero_falsy_not() {
    let out = compile_and_run(r#"<?php echo !"0" ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

/// Verifies that a non-empty string is truthy.
#[test]
fn test_string_nonempty_truthy() {
    let out = compile_and_run(r#"<?php echo "hello" ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

/// Verifies that an empty string is falsy.
#[test]
fn test_string_empty_falsy() {
    let out = compile_and_run(r#"<?php echo "" ? "yes" : "no";"#);
    assert_eq!(out, "no");
}

// --- Bug fix: compound assignment in for-loop update ---

/// Verifies the shared mbregex engine anchors matching at the subject start.
/// PHP 8.5 treats the operation as a prefix
/// match at offset 0 (`'ab'`/`'abc'` = true, `'bc'`/`'abc'` = false); `\z` forces end-anchoring.
#[test]
fn test_mb_ereg_match_start_anchored() {
    let out = compile_and_run(
        "<?php echo (int)mb_ereg_match('ab','abc'), (int)mb_ereg_match('bc','abc'), (int)mb_ereg_match('^[A-Z][A-Za-z0-9]*$','Foo'), (int)mb_ereg_match('[a-z]+\\z','abc123');",
    );
    assert_eq!(out, "1010");
}

/// Verifies that `mb_ereg_match()` accepts the optional options argument and honors `i`.
#[test]
fn test_mb_ereg_match_options_case_insensitive() {
    let out = compile_and_run(
        "<?php echo (int)mb_ereg_match('ab','AB'), (int)mb_ereg_match('ab','AB','i'), (int)mb_ereg_match('ab','AB', null);",
    );
    assert_eq!(out, "010");
}


/// Verifies the `/x` (extended) modifier drops literal whitespace and `#` comments from the
/// pattern instead of matching them.
///
/// `__rt_preg_strip` scanned trailing modifiers for `i`, `m`, `s`, `u`, `U` and `A` and silently
/// ignored everything else, so an extended pattern was compiled with its layout as pattern text
/// and simply never matched -- `preg_match('/ a b /x', 'ab')` answered 0 where PHP answers 1. The
/// shim already mapped `ELEPHC_PCRE2_CFLAG_EXTENDED`; only the modifier byte was missing.
///
/// Symfony's `HeaderUtils::split()` writes its whole pattern in extended form, which is why the
/// compiled `--web` response answered `Cache-Control: , private`.
///
/// Oracle: `php -n` 8.5.10 prints the asserted line.
#[test]
fn test_extended_modifier_ignores_pattern_layout() {
    let out = compile_and_run(
        r#"<?php
echo preg_match('/ a b /x', 'ab'), '|';
echo preg_match('/a # trailing comment
b/x', 'ab'), '|';
echo preg_match('/[ab ]+/x', 'a b'), '|';
echo preg_match('/ a b /', ' a b '), '|';
echo preg_replace('/\s+/x', '-', 'p q'), '|';
echo preg_match('/x(?<=x)y/x', 'xy');
"#,
    );
    assert_eq!(out, "1|1|1|1|p-q|1");
}

/// Verifies `preg_match()` fills `$matches` with PHP's ordered hash when the pattern declares a
/// capture name: each name immediately before its numeric twin.
///
/// The runtime already built exactly that (`__rt_preg_match_capture_named`). The CHECKER typed the
/// destination as an indexed array with a `Mixed` element, on the belief that `Array(Mixed)` reads
/// both key kinds back out. It does not -- consumers read the static type -- so the hash was read
/// as a packed vector and `preg_match('/(?<word>[a-z]+)(?<num>[0-9]+)/', 'a1', $n)` answered
/// `[0,4,1,0,-1]`, raw slot words rather than captures.
///
/// All three PCRE spellings are covered, plus the two that must NOT count: `(?<=` is lookbehind,
/// not a name, and an unmatched named group still gets both keys with an empty string.
///
/// Oracle: `php -n` 8.5.10 prints the asserted line.
#[test]
fn test_preg_match_named_groups_fill_an_ordered_hash() {
    let out = compile_and_run(
        r#"<?php
preg_match('/(?<word>[a-z]+)(?<num>[0-9]+)/', 'a1', $n);
echo json_encode($n), '|';
preg_match("/(?P<host>[a-z]+)\.(?P<tld>[a-z]+)/", 'site.dev', $p);
echo json_encode($p), '|';
preg_match("/(?'k'[a-z]+)/", 'zz', $q);
echo json_encode($q), '|';
preg_match('/(?<=a)(b)/', 'ab', $look);
echo json_encode($look), '|';
echo preg_match('/(?<miss>z)|(y)/', 'y', $partial), ' ', json_encode($partial);
"#,
    );
    assert_eq!(
        out,
        concat!(
            r#"{"0":"a1","word":"a","1":"a","num":"1","2":"1"}|"#,
            r#"{"0":"site.dev","host":"site","1":"site","tld":"dev","2":"dev"}|"#,
            r#"{"0":"zz","k":"zz","1":"zz"}|["b","b"]|1 "#,
            r#"{"0":"y","miss":"","1":"","2":"y"}"#,
        ),
    );
}


/// Verifies leading whitespace in front of the pattern delimiter is skipped, as php-src does.
///
/// `php_pcre_get_compiled_regex_cache` advances past isspace() bytes before reading the delimiter.
/// `__rt_preg_strip` read the first byte directly, so a pattern OPENING with a newline looked
/// undelimited: PCRE2 then received the delimiters and modifiers as pattern text and never
/// matched, silently. Symfony's `HeaderUtils::split()` writes exactly that shape -- an extended
/// pattern laid out across lines -- which is how a `Cache-Control` header stopped parsing.
///
/// The last case pins that the skip does not assume '/': `#...#` still delimits after whitespace.
///
/// Oracle: `php -n` 8.5.10 prints the asserted line.
#[test]
fn test_leading_whitespace_before_the_pattern_delimiter_is_skipped() {
    let out = compile_and_run(
        r#"<?php
echo preg_match("\n  /ab/", 'xaby'), '|';
echo preg_match("\t/ab/i", 'xABy'), '|';
echo preg_replace("\n/a/", 'Z', 'aaa'), '|';
echo preg_match('
    /
        a b   # a comment
    /x', 'ab'), '|';
echo preg_match(" \n\t\r#ab#", 'zaby');
"#,
    );
    assert_eq!(out, "1|1|ZZZ|1|1");
}


/// A `$matches` destination opened as `[]` must still be able to hold the match array.
///
/// `$m = []` types the local `array<never>` -- an element type no written value satisfies -- and
/// the by-reference write never widened the frame storage, so the backend refused the destination
/// outright with `preg_match matches destination PHP type Array(Never)` rather than miscompiling.
/// Symfony's `UrlMatcher::matchCollection` opens `$hostMatches = []` exactly like that, which kept
/// the whole routing component out of the compiled world.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_preg_match_destination_opened_as_an_empty_array_literal() {
    let out = compile_and_run_with_regex(
        r#"<?php
function matchIt(string $subject): array {
    $m = [];
    if (!preg_match('/^(\w+)-(\d+)$/', $subject, $m)) {
        return [];
    }
    return $m;
}
echo implode(',', matchIt('abc-42')), '|', count(matchIt('nope'));
"#,
    );
    assert_eq!(out, "abc-42,abc,42|0");
}

/// A named-capture pattern keeps its HASH destination even when the local opened as `[]`.
///
/// The widening above must reuse the checker's own shape decision rather than make a second one:
/// stamping an indexed type onto a destination the runtime fills with a hash reads it back as a
/// renumbered list.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_preg_match_named_captures_into_an_empty_array_literal_destination() {
    let out = compile_and_run_with_regex(
        r#"<?php
$m = [];
preg_match('/(?<w>[a-z]+)(?<n>[0-9]+)/', 'a1', $m);
echo $m['w'], '|', $m['n'], '|', $m[1], '|', $m[2];
"#,
    );
    assert_eq!(out, "a|1|a|1");
}


/// `is_callable()` on an unconstrained value proves a callable, which in PHP may be a string.
///
/// The guard narrowed to the descriptor-shaped `Callable`, whose representation is not the boxed
/// cell the storage actually holds, so handing the value to a `string` parameter was refused:
/// "parameter $identifier expects Str, got Callable". The narrowed type is now the boxed
/// `callable|string` union, which the scalar funnel accepts and which keeps the storage it has.
///
/// `Symfony\Component\VarDumper\Caster\ClassStub::wrapCallable` is the shape.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_is_callable_narrowing_still_reaches_a_string_parameter() {
    let out = compile_and_run(
        r#"<?php
class Stub {
    public string $value;
    public function __construct(string $identifier, callable|array|string|null $callable = null) {
        $this->value = $identifier;
    }
}
function wrap(mixed $callable): string {
    if (\is_object($callable) || !\is_callable($callable)) {
        return 'skip';
    }
    if (!\is_array($callable)) {
        $stub = new Stub($callable, $callable);
        return 'string:'.$stub->value;
    }
    return 'array';
}
echo wrap('strlen'), '/', wrap(42), '/', wrap(static fn (): int => 1);
"#,
    );
    assert_eq!(out, "string:strlen/skip/skip");
}


/// PCRE's MARK verb reaches `$matches['MARK']`, compiled and interpreted alike.
///
/// Symfony's dumped `CompiledUrlMatcher` marks each alternative of its dynamic-route regexp with
/// `(*:<offset>)` and selects the branch with `$this->dynamicRoutes[(int) $matches['MARK']]`.
/// elephc produced no such key, so the index was `(int) null` = 0, `$dynamicRoutes[0]` was null,
/// and `foreach (null as ...)` fatalled — every route with a placeholder, `/greet/{name}` included.
///
/// Four places had to move together and any one of them left the key silently absent:
/// `elephc_pcre2_v1_last_mark` in the pcre2 shim (with a RECIPE REVISION BUMP, because the catalog
/// cache key does not hash the shim source), `__rt_preg_match_capture` for `preg_match()`,
/// `__rt_preg_match_row` for `preg_match_all()`, and the eval bridge's own provider table — the
/// matcher runs interpreted, so the compiled half alone changes nothing visible.
///
/// A mark forces the HASH row for the same reason a declared group name does: `MARK` is a string
/// key. The fixture therefore also pins the shapes that must NOT become hashes.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_pcre_mark_verb_reaches_the_matches_array() {
    let out = compile_and_run_with_regex(
        r#"<?php
$re = '{^(?|/greet/([^/]++)(*:22)|/other/([^/]++)(*:44))/?$}sDu';
preg_match($re, '/greet/Bob', $a);
preg_match($re, '/other/Ann', $b);
preg_match('{^/plain$}', '/plain', $c);
preg_match('{^/(?<who>\w+)$}', '/Bob', $d);
preg_match_all('{(a)(*:A)|(b)(*:B)}', 'ab', $e, PREG_SET_ORDER);
echo json_encode($a), "\n", json_encode($b), "\n", json_encode($c), "\n", json_encode($d), "\n", json_encode($e), "\n";
eval('
preg_match($re, "/greet/Bob", $a2);
preg_match($re, "/greet/Bob", $b2, PREG_OFFSET_CAPTURE);
preg_match("{^/plain$}", "/plain", $c2);
echo json_encode($a2), "\n", json_encode($b2), "\n", json_encode($c2), "\n";
');
"#,
    );
    assert_eq!(
        out,
        concat!(
            "{\"0\":\"\\/greet\\/Bob\",\"1\":\"Bob\",\"MARK\":\"22\"}\n",
            "{\"0\":\"\\/other\\/Ann\",\"1\":\"Ann\",\"MARK\":\"44\"}\n",
            "[\"\\/plain\"]\n",
            "{\"0\":\"\\/Bob\",\"who\":\"Bob\",\"1\":\"Bob\"}\n",
            "[{\"0\":\"a\",\"1\":\"a\",\"MARK\":\"A\"},{\"0\":\"b\",\"1\":\"\",\"2\":\"b\",\"MARK\":\"B\"}]\n",
            "{\"0\":\"\\/greet\\/Bob\",\"1\":\"Bob\",\"MARK\":\"22\"}\n",
            "{\"0\":[\"\\/greet\\/Bob\",0],\"1\":[\"Bob\",7],\"MARK\":\"22\"}\n",
            "[\"\\/plain\"]\n",
        )
    );
}

/// `PREG_OFFSET_CAPTURE` and `PREG_PATTERN_ORDER` carry the MARK verb as well: a plain string key
/// on a single match, and a `MARK` column indexed by match number when several match.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_pcre_mark_verb_reaches_offset_capture_and_pattern_order_arrays() {
    let out = compile_and_run_with_regex(
        r#"<?php
preg_match('{(a)(*:A)|(b)(*:B)}', 'ab', $a, PREG_OFFSET_CAPTURE);
preg_match_all('{(a)(*:A)|(b)(*:B)}', 'ab', $b, PREG_PATTERN_ORDER | PREG_OFFSET_CAPTURE);
preg_match_all('{(a)(*:A)|(b)}', 'ab', $c, PREG_PATTERN_ORDER | PREG_OFFSET_CAPTURE);
echo json_encode($a), "\n", json_encode($b), "\n", json_encode($c), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "{\"0\":[\"a\",0],\"1\":[\"a\",0],\"MARK\":\"A\"}\n",
            "{\"0\":[[\"a\",0],[\"b\",1]],\"1\":[[\"a\",0],[\"\",-1]],\"2\":[[\"\",-1],[\"b\",1]],\"MARK\":[\"A\",\"B\"]}\n",
            "{\"0\":[[\"a\",0],[\"b\",1]],\"1\":[[\"a\",0],[\"\",-1]],\"2\":[[\"\",-1],[\"b\",1]],\"MARK\":[\"A\"]}\n",
        )
    );
}

/// A `$matches` destination that ALREADY HOLDS A STRING, which php simply overwrites.
///
/// The by-reference write never widened the destination's FRAME STORAGE, so the backend refused
/// the destination outright with `unsupported EIR backend feature: preg_match matches destination
/// PHP type Str`. `ir_lower`'s `prepare_regex_match_output_local` had the widening for exactly one
/// prior shape (`$m = []`, i.e. `array<never>`, from Symfony's `UrlMatcher::matchCollection`); a
/// destination holding any other non-array type reached the backend unwidened.
///
/// The SECOND function is the shape that mattered in real code, and it is why the widening cannot
/// simply re-type the local to the match array: `$s1` is both the SUBJECT and the DESTINATION of
/// the same call, so it is still read as a string one operand before it is overwritten.
/// `Symfony\Polyfill\Php85\Php85::grapheme_levenshtein` writes it verbatim as
/// `preg_match_all('/'.$regex.'/u', $s1, $s1);`.
///
/// Oracle: `php 8.5.10` prints `a,b,c` and `x,y,z`.
#[test]
fn test_preg_match_all_destination_that_already_holds_a_string() {
    let out = compile_and_run_with_regex(
        r#"<?php
function local_dest(string $in): string {
    $m = "seed";
    preg_match_all('/[a-z]/', $in, $m);
    return implode(",", $m[0]);
}
function param_dest(string $s1): string {
    preg_match_all('/[a-z]/', $s1, $s1);
    return implode(",", $s1[0]);
}
echo local_dest("abc"), "\n", param_dest("xyz"), "\n";
"#,
    );
    assert_eq!(out, "a,b,c\nx,y,z\n");
}

/// Verifies a global search (`preg_match_all`, `preg_replace`, `preg_replace_callback`) keeps
/// the WHOLE subject visible when it continues after a match.
///
/// Every continuation used to hand PCRE2 the remaining suffix as a fresh subject, so `\b`,
/// lookbehind and `^` under `m` treated the cursor as the start of the string:
/// `preg_match_all('/\b\w/', 'hello world')` counted all ten letters and `preg_replace('/^./m')`
/// rewrote every character. The runtime now searches the whole subject from a start offset, as
/// php does.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_global_preg_search_sees_the_bytes_before_its_cursor() {
    let out = compile_and_run_with_regex(
        r#"<?php
$s = "hello world";
echo preg_match_all('/\b\w/', $s), "|";
echo preg_match_all('/\b\w/', $s, $m), ":", implode(",", $m[0]), "|";
echo preg_match_all('/\b\w/', $s, $m, 0, 3), ":", implode(",", $m[0]), "\n";
echo preg_replace('/\b\w/', 'X', $s), "|";
echo preg_replace_callback('/\b\w/', fn($m) => strtoupper($m[0]), $s), "|";
echo preg_replace('/(?<=a)./', 'X', "aaa"), ":", preg_match_all('/(?<=a)./', "aaa"), "\n";
$c = "a1\nb2\nc3";
echo preg_replace('/^./m', '<$0>', $c), "|", preg_match_all('/^./m', $c), "|";
echo preg_replace_callback('/^\w/m', fn($m) => "[" . $m[0] . "]", $c), "\n";
"#,
    );
    assert_eq!(
        out,
        "2|2:h,w|1:w\nXello Xorld|Hello World|aXX:2\n<a>1\n<b>2\n<c>3|3|[a]1\n[b]2\n[c]3\n"
    );
}

/// Verifies `preg_replace_callback()` with a function NAME calls that function whatever its
/// parameter and return shape.
///
/// A literal name was called straight at the function's entry with the bare match array, which
/// only fits a function taking `array<string>` and returning `string`. An untyped
/// `function up($m)` keeps a boxed `mixed` parameter, so it read the array pointer as a box and
/// crashed; such a function now goes through the callable wrapper that boxes the argument.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_preg_replace_callback_calls_a_named_function_of_any_shape() {
    let out = compile_and_run_with_regex(
        r#"<?php
function up($m) { return strtoupper($m[0]); }
function bracket(array $m): string { return "[" . $m[0] . "]"; }
function count_of($m) { return strlen($m[0]); }
echo preg_replace_callback('/l+/', 'up', "hello world"), "|";
echo preg_replace_callback('/o/', 'bracket', "foo"), "|";
echo preg_replace_callback('/l+/', 'count_of', "hello world"), "\n";
"#,
    );
    assert_eq!(out, "heLLo worLd|f[o][o]|he2o wor1d\n");
}

/// `preg_match_all()` starts its search at the byte `$offset` and reports capture offsets
/// relative to the whole subject, in count-only, PATTERN_ORDER, SET_ORDER and OFFSET_CAPTURE
/// forms; a negative offset counts from the end and one past the end fails.
///
/// Oracle: `php -n` 8.5.10.
#[test]
fn test_preg_match_all_honours_its_start_offset_in_every_form() {
    let out = compile_and_run_with_regex(
        r#"<?php
$s = "hello world foo";
echo preg_match_all('/\w+/', $s, $m, PREG_PATTERN_ORDER, 3), ":", implode(",", $m[0]), "\n";
echo preg_match_all('/\w+/', $s, $m, PREG_SET_ORDER, 6), ":", implode(",", array_map(fn($x) => $x[0], $m)), "\n";
echo preg_match_all('/(\w)(\w)/', $s, $m, PREG_PATTERN_ORDER | PREG_OFFSET_CAPTURE, 12), ":", json_encode($m), "\n";
echo preg_match_all('/\w+/', $s, $m, PREG_PATTERN_ORDER | PREG_OFFSET_CAPTURE, 3), ":", json_encode($m), "\n";
echo preg_match_all('/^\w/', $s, $m, 0, 3), ":", json_encode($m), "\n";
echo preg_match_all('/\w+/', $s, $m, 0, -3), ":", json_encode($m), "\n";
echo preg_match_all('/o/', $s, $m, 0, 5), ":", json_encode($m), "\n";
echo preg_match_all('/\w+/', $s, $m, offset: 3), ":", implode(",", $m[0]), "\n";
echo preg_match_all('/\w+/', $s, $m, flags: PREG_SET_ORDER, offset: 6), ":", json_encode($m), "\n";
echo preg_match_all('/\w+/', $s, $m, flags: PREG_OFFSET_CAPTURE, offset: -3), ":", json_encode($m), "\n";
echo preg_match_all('/^\w/', $s, $m, offset: 3), ":", json_encode($m), "\n";
echo preg_match_all('/\b\w/', $s, $m, offset: 3), ":", json_encode($m), "\n";
preg_match_all('/\w+/', $s, $m, offset: 99);
echo json_encode($m), "\n";
echo preg_match_all('/\w+/', $s, $m, offset: 15), ":", json_encode($m), "\n";
"#,
    );
    assert_eq!(
        out,
        "3:lo,world,foo\n2:world,foo\n1:[[[\"fo\",12]],[[\"f\",12]],[[\"o\",13]]]\n3:[[[\"lo\",3],[\"world\",6],[\"foo\",12]]]\n0:[[]]\n1:[[\"foo\"]]\n3:[[\"o\",\"o\",\"o\"]]\n3:lo,world,foo\n2:[[\"world\"],[\"foo\"]]\n1:[[[\"foo\",12]]]\n0:[[]]\n2:[[\"w\",\"f\"]]\n[]\n0:[[]]\n"
    );
}
