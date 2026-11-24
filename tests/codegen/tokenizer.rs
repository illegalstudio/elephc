//! Purpose:
//! End-to-end PHP tokenizer coverage for functions, PhpToken, callables and runtime eval.
//!
//! Called from:
//! - The codegen CI matrix on every executable supported host target.
//!
//! Key details:
//! - The Rust engine's differential corpus checks the full PHP token vocabulary.
//! - These fixtures exercise PHP storage, exception handling, late-static construction and GC.

use crate::support::*;

/// token_get_all returns the PHP mixture of named tuples and punctuation strings.
#[test]
fn test_tokenizer_mixed_stream_and_roundtrip() {
    let out = compile_and_run(r#"<?php
$code = '<?php echo 1;';
$tokens = token_get_all($code);
echo count($tokens), ':', token_name($tokens[1][0]), ':', $tokens[4], "\n";
$restored = '';
foreach ($tokens as $token) { $restored .= is_array($token) ? $token[1] : $token; }
echo $restored === $code ? 'same' : 'different';
echo ':', token_name(59), ':', token_name(-1), ':', token_name(T_PAAMAYIM_NEKUDOTAYIM);
"#);
    assert_eq!(out, "5:T_ECHO:;\nsame:UNKNOWN:UNKNOWN:T_DOUBLE_COLON");
}

/// PhpToken retains its public fields, default constructor coordinates and strict predicates.
#[test]
fn test_tokenizer_phptoken_members_and_subclasses() {
    let out = compile_and_run(r#"<?php
$t = new PhpToken(T_STRING, 'name');
echo $t->getTokenName(), ':', $t->line, ':', $t->pos, ':', (string)$t, "\n";
echo $t->is(T_STRING) ? '1' : '0';
echo $t->is('name') ? '1' : '0';
echo $t->is(['other', T_STRING]) ? '1' : '0';
echo $t->is([]) ? '1' : '0';
echo $t->isIgnorable() ? '1' : '0';
$t->id = T_COMMENT;
echo $t->isIgnorable() ? '1' : '0';
$t->text = 'changed';
echo ':', $t, "\n";
$unknown = new PhpToken(999999, 'x');
echo $unknown->getTokenName() === null ? 'null' : 'wrong';
$char = new PhpToken(59, ';');
echo ':', $char->getTokenName();
class SourceToken extends PhpToken { public function upper(): string { return strtoupper($this->text); } }
$items = SourceToken::tokenize('<?php echo 1;');
echo ':', get_class($items[1]), ':', $items[1]->upper();
"#);
    assert_eq!(out, "T_STRING:-1:-1:name\n111001:changed\nnull:;:SourceToken:ECHO");
}

/// Byte positions count UTF-8 bytes and CRLF line breaks, independently of token representation.
#[test]
fn test_tokenizer_phptoken_byte_positions_and_lines() {
    let out = compile_and_run(r#"<?php
$code = "<?php\r\n" . '$x="è";' . "\r\n?>\r\nhtml";
$tokens = PhpToken::tokenize($code);
echo $tokens[1]->pos, ':', $tokens[1]->line, ':', $tokens[1]->text, "\n";
$restored = '';
foreach ($tokens as $token) { $restored .= $token->text; }
echo $restored === $code ? 'same' : 'different';
$last = $tokens[count($tokens) - 1];
echo ':', $last->line, ':', $last->pos, ':', $last->text;
"#);
    assert_eq!(out, "7:2:$x\nsame:4:21:html");
}

/// TOKEN_PARSE validates syntax and applies reserved identifier feedback while plain mode is permissive.
#[test]
fn test_tokenizer_parse_flag_and_catchable_errors() {
    let out = compile_and_run(r#"<?php
$flags = $argc > 0 ? TOKEN_PARSE : 0;
$source = '<?php class A { public const PUBLIC = 1; }';
$tokens = token_get_all($source, $flags);
foreach ($tokens as $t) {
    if (is_array($t) && $t[1] === 'PUBLIC') { echo token_name($t[0]), "\n"; }
}
try { token_get_all('<?php function broken(', $flags); echo 'wrong'; }
catch (ParseError $error) { echo 'ParseError', "\n"; }
echo count(token_get_all('<?php function broken(')) > 0 ? 'permissive' : 'wrong';
"#);
    assert_eq!(out, "T_STRING\nParseError\npermissive");
}

/// TOKEN_PARSE throws CompileError for a nested halt and ParseError for the other halt failures.
///
/// Direct calls use the compiled prelude. The literal eval uses Magician's tokenizer throw.
#[test]
fn test_tokenizer_halt_compiler_exception_class() {
    let out = compile_and_run(r#"<?php
function show(string $label, string $code): void {
    try {
        token_get_all($code, TOKEN_PARSE);
        echo $label, ":ok\n";
    } catch (Throwable $e) {
        echo $label, ":", get_class($e), ":", $e->getMessage(), "\n";
    }
}
show('fn', '<?php function f() { __halt_compiler(); }');
show('ns', '<?php namespace N { __halt_compiler(); }');
show('if', '<?php if (1) __halt_compiler();');
show('top', '<?php __halt_compiler();');
try {
    PhpToken::tokenize('<?php function p() { __halt_compiler(); }', TOKEN_PARSE);
    echo "token:ok\n";
} catch (ParseError $e) {
    echo "token:ParseError\n";
} catch (CompileError $e) {
    echo "token:CompileError\n";
}
eval('try { token_get_all("<?php function g() { __halt_compiler(); }", TOKEN_PARSE); echo "eval:ok\n"; } catch (ParseError $e) { echo "eval:ParseError\n"; } catch (CompileError $e) { echo "eval:CompileError\n"; }');
"#);
    assert_eq!(
        out,
        "fn:CompileError:__HALT_COMPILER() can only be used from the outermost scope\n\
ns:ParseError:Unclosed '{'\n\
if:ParseError:syntax error, unexpected token \"__halt_compiler\"\n\
top:ok\n\
token:CompileError\n\
eval:CompileError\n"
    );
}

/// A nested unpack contributes an unknown number of arguments, so the static arity gate
/// leaves the call to the runtime instead of counting the unpack itself as one argument.
#[test]
fn test_tokenizer_nested_spread_argument_count() {
    let out = compile_and_run(r#"<?php
$empty = [];
$tokens = token_get_all('<?php echo 1;', ...[0, ...$empty]);
echo is_array($tokens) ? 'ok' : 'bad';
"#);
    assert_eq!(out, "ok");
}

/// Named, spread, case-insensitive and first-class callable forms share PHP's signatures.
#[test]
fn test_tokenizer_callable_and_named_argument_forms() {
    let out = compile_and_run(r#"<?php
$scan = \TOKEN_GET_ALL(...);
$name = token_name(...);
$tokens = $scan(flags: 0, code: '<?php echo 1;');
echo $name(id: $tokens[1][0]), ':', count(token_get_all(...['code' => 'html', 'flags' => 0]));
$f = $argc > 0 ? 'token_name' : 'strval';
echo ':', $f(T_FUNCTION);
"#);
    assert_eq!(out, "T_ECHO:1:T_FUNCTION");
}

/// Opaque eval reaches both function adapters and the host PhpToken class, including callables.
#[test]
fn test_tokenizer_runtime_eval_bundle() {
    let out = compile_and_run(r#"<?php
$code = $argc > 0 ? '
$t = token_get_all("<?php echo 1;");
echo token_name($t[1][0]), ":", count($t), "\n";
$f = "token_name"; echo $f(T_FUNCTION), "\n";
$p = PhpToken::tokenize("<?php echo 1;");
echo get_class($p[1]), ":", $p[1]->text, ":", $p[1]->pos, "\n";
try { token_get_all("<?php function broken(", TOKEN_PARSE); }
catch (ParseError $e) { echo "ParseError"; }
abstract class EvalAbstractSourceToken extends PhpToken {}
try { EvalAbstractSourceToken::tokenize(""); echo "wrong"; }
catch (Error $e) { echo ":abstract"; }
class EvalSourceToken extends PhpToken {}
$children = EvalSourceToken::tokenize("html");
echo ":", get_class($children[0]), ":", $children[0]->text;
class EvalTokenSource { public function __toString(): string { return "html"; } }
echo ":", count(token_get_all(new EvalTokenSource()));
foreach (["token_get_all", "token_name"] as $function) {
    try { $function([]); echo "wrong"; }
    catch (TypeError $e) { echo ":TypeError"; }
}
' : '';
eval($code);
"#);
    assert_eq!(out, "T_ECHO:5\nT_FUNCTION\nPhpToken:echo:6\nParseError:abstract:EvalSourceToken:html:1:TypeError:TypeError");
}

/// Repeated token arrays leave no live PHP allocations after scope cleanup.
#[test]
fn test_tokenizer_repeated_arrays_are_released() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function inspect_source(): void {
    for ($i = 0; $i < 20; $i++) {
        $tokens = token_get_all('<?php echo "hello";');
    }
}
inspect_source();
echo 'done';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Repeated object factories release their intermediate token arrays and boxed property values.
#[test]
fn test_tokenizer_repeated_objects_are_released() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function inspect_tokens(): void {
    for ($i = 0; $i < 20; $i++) {
        $objects = PhpToken::tokenize('<?php echo "hello";');
    }
}
inspect_tokens();
echo 'done';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Predicates reject invalid dynamic kinds without validating entries after a successful match.
#[test]
fn test_tokenizer_is_checks_dynamic_types_in_source_order() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
$t = new PhpToken(T_STRING, 'name');
$values = [true, 1.5, null, new stdClass()];
foreach ($values as $value) {
    try { $t->is($value); echo 'wrong'; }
    catch (TypeError $error) { echo str_contains($error->getMessage(), '($kind)') ? 'T' : 'wrong'; }
}
echo ':', $t->is(['name', []]) ? 'match' : 'wrong';
try { $t->is([[], 'name']); echo 'wrong'; }
catch (TypeError $error) { echo ':TypeError'; }
"#);
    assert!(out.success, "{}\n{}\n{}", out.stdout, out.stderr, asm);
    assert_eq!(out.stdout, "TTTT:match:TypeError");
}

/// Empty token streams still reject abstract subclasses, and partial binary strings retain quote IDs.
#[test]
fn test_tokenizer_abstract_subclass_and_binary_quote() {
    let out = compile_and_run(r#"<?php
abstract class AbstractSourceToken extends PhpToken {}
try { AbstractSourceToken::tokenize(''); echo 'wrong'; }
catch (Error $error) { echo 'abstract'; }
$tokens = PhpToken::tokenize('<?php b"unfinished');
echo ':', $tokens[1]->id, ':', $tokens[1]->text;
"#);
    assert_eq!(out, "abstract:34:b\"");
}

/// Native tokenization errors retain the scanned buffer's line and explicitly empty filename.
#[test]
fn test_tokenizer_parse_error_source_locations() {
    let out = compile_and_run(r#"<?php
$source = "<?php\n\nfunction broken(";
try { token_get_all($source, TOKEN_PARSE); echo 'wrong'; }
catch (ParseError $error) {
    echo $error->getLine(), ':', strlen($error->getFile()), "\n";
}
try { PhpToken::tokenize($source, TOKEN_PARSE); echo 'wrong'; }
catch (ParseError $error) { echo $error->getLine(), ':', strlen($error->getFile()); }
"#);
    assert_eq!(out, "3:0\n3:0");
}

/// Opaque eval enforces internal integer ranges before casts and preserves numeric diagnostics.
#[test]
fn test_tokenizer_runtime_eval_scalar_validation_bundle() {
    let out = compile_and_run_capture(r#"<?php
class NativeCheckedToken extends PhpToken {}
$code = $argc > 0 ? '
class EvalCheckedToken extends PhpToken {}
foreach ([1e100, -1e100, NAN, INF, -INF, "1e100", "-1e100", "9223372036854775808"] as $bad) {
    try { token_name($bad); echo "wrong"; } catch (TypeError $error) { echo "T"; }
    try { token_get_all("html", $bad); echo "wrong"; } catch (TypeError $error) { echo "T"; }
    try { PhpToken::tokenize("html", $bad); echo "wrong"; } catch (TypeError $error) { echo "T"; }
    try { NativeCheckedToken::tokenize("html", flags: $bad); echo "wrong"; } catch (TypeError $error) { echo "T"; }
    try { EvalCheckedToken::tokenize("html", $bad); echo "wrong"; } catch (TypeError $error) { echo "T"; }
    echo "\n";
}
echo token_name("9223372036854775807"), ":", token_name("-9223372036854775808"), ":", token_name("1e2"), "\n";
echo token_name("1.5"), ":", count(token_get_all("html", "1.5")), ":", count(PhpToken::tokenize("html", "1.5")), "\n";
echo token_name("\v1"), ":", token_name("1\v"), ":", token_name("\v1\v"), ":", count(token_get_all("html", "\v0\v")), ":", count(PhpToken::tokenize("html", "\v0\v")), "\n";
echo token_name("\v1.5\v"), ":", count(token_get_all("html", "\v1.5\v")), ":", count(PhpToken::tokenize("html", "\v1.5\v")), "\n";
echo count(token_get_all(null)), ":", token_name(null), ":", count(token_get_all("html", null)), ":", count(PhpToken::tokenize(null)), "\n";
foreach (["token_get_all", "PhpToken::tokenize", "NativeCheckedToken::tokenize", "EvalCheckedToken::tokenize"] as $factory) {
    try { $factory("<?php\r\n\r\nfunction broken(", TOKEN_PARSE); echo "wrong"; }
    catch (ParseError $error) { echo $error->getLine(), ":", strlen($error->getFile()), "\n"; }
}
' : '';
eval($code);
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}UNKNOWN:UNKNOWN:UNKNOWN\nUNKNOWN:1:1\nUNKNOWN:UNKNOWN:UNKNOWN:1:1\nUNKNOWN:1:1\n0:UNKNOWN:1:0\n3:0\n3:0\n3:0\n3:0\n", "TTTTT\n".repeat(8)));
    assert_eq!(out.stderr.matches("Implicit conversion from float-string \"1.5\" to int loses precision").count(), 3, "{}", out.stderr);
    assert_eq!(out.stderr.matches("Implicit conversion from float-string \"\u{b}1.5\u{b}\" to int loses precision").count(), 3, "{}", out.stderr);
    assert_eq!(out.stderr.matches("Passing null to parameter").count(), 4, "{}", out.stderr);
}

/// Verifies the literal loop-invariant token_get_all count hoist and a dynamic fallback.
#[test]
fn test_token_get_all_literal_count_hoist() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$s = "<?php echo 1;";
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= count(token_get_all($s)) + $i; }
echo $n, "|";
$dynamic = "<?php echo " . $argc . ";";
echo count(token_get_all($dynamic)), "|";
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "4|5|\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// TOKEN_PARSE reports PHP's E_DEPRECATED level for every non-canonical cast spelling.
#[test]
fn test_tokenizer_non_canonical_cast_deprecations() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message): bool {
    echo $level, ':', $message, "\n";
    return true;
});
foreach (['(double)', '(integer)', '(boolean)', '(binary)', '(float)', '(int)'] as $cast) {
    token_get_all('<?php ' . $cast . ' $x;', TOKEN_PARSE);
}
PhpToken::tokenize('<?php (double) $x;', TOKEN_PARSE);
"#);
    assert_eq!(
        out,
        "8192:Non-canonical cast (double) is deprecated, use the (float) cast instead\n8192:Non-canonical cast (integer) is deprecated, use the (int) cast instead\n8192:Non-canonical cast (boolean) is deprecated, use the (bool) cast instead\n8192:Non-canonical cast (binary) is deprecated, use the (string) cast instead\n8192:Non-canonical cast (double) is deprecated, use the (float) cast instead\n"
    );
}

/// `declare(encoding=...)` reports through PHP's E_WARNING channel, not the cast deprecations.
#[test]
fn test_tokenizer_declare_encoding_uses_warning_level() {
    let out = compile_and_run_capture(r#"<?php
token_get_all('<?php declare(encoding="ISO-8859-1");', TOKEN_PARSE);
"#);
    assert!(out.success, "{}", out.stderr);
    assert!(
        out.stderr.contains(
            "Warning: declare(encoding=...) ignored because Zend multibyte feature is turned off by settings"
        ),
        "{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("Deprecated: declare(encoding=...)"),
        "{}",
        out.stderr
    );
}

/// The `declare(encoding=...)` warning survives the parse failure that follows it.
#[test]
fn test_tokenizer_declare_encoding_warning_precedes_parse_error() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo $level, ':', $line, "\n";
    return true;
});
try {
    token_get_all("<?php declare(encoding='ISO-8859-1'); echo ;", TOKEN_PARSE);
} catch (ParseError $e) {
    echo 'caught:', $e->getLine(), "\n";
}
"#);
    assert_eq!(out, "2:1\ncaught:1\n");
}

/// Runtime eval reports `declare(encoding=...)` through the Warning channel too.
#[test]
fn test_tokenizer_eval_declare_encoding_warning_level() {
    let out = compile_and_run_capture(r#"<?php
$code = $argc > 0 ? '
token_get_all("<?php declare(encoding=\"ISO-8859-1\");", TOKEN_PARSE);
' : '';
eval($code);
"#);
    assert!(out.success, "{}", out.stderr);
    assert!(
        out.stderr.contains(
            "Warning: declare(encoding=...) ignored because Zend multibyte feature is turned off by settings"
        ),
        "{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("Deprecated: declare(encoding=...)"),
        "{}",
        out.stderr
    );
}

/// A throwing eval error handler stays catchable across a tokenizer warning.
#[test]
fn test_tokenizer_eval_warning_handler_throw_is_catchable() {
    let out = compile_and_run(r#"<?php
$code = $argc > 0 ? '
try {
    set_error_handler(function (int $level, string $message): bool {
        throw new RuntimeException("handled:" . $level);
    });
    token_get_all("<?php declare(encoding=\"ISO-8859-1\");", TOKEN_PARSE);
    echo "no throw";
} catch (RuntimeException $e) {
    echo $e->getMessage();
}
' : '';
eval($code);
"#);
    assert_eq!(out, "handled:2");
}

/// Runtime eval raises the same non-canonical-cast deprecations through TOKEN_PARSE.
#[test]
fn test_tokenizer_eval_non_canonical_cast_deprecations() {
    let out = compile_and_run(r#"<?php
$code = $argc > 0 ? '
set_error_handler(function (int $level, string $message): bool {
    echo $level, ":", $message, "\n";
    return true;
});
token_get_all("<?php (double) \$x;", TOKEN_PARSE);
PhpToken::tokenize("<?php (binary) \$x;", TOKEN_PARSE);
' : '';
eval($code);
"#);
    assert_eq!(
        out,
        "8192:Non-canonical cast (double) is deprecated, use the (float) cast instead\n\
8192:Non-canonical cast (binary) is deprecated, use the (string) cast instead\n"
    );
}

/// PhpToken tokens come from a Mixed array element; unset must dispatch on the runtime class.
#[test]
fn test_tokenizer_phptoken_unset_mixed_receiver_property() {
    let out = compile_and_run(r#"<?php
$tokens = PhpToken::tokenize('<?php function foo() {}');
$token = $tokens[1];
unset($token->id);
unset($token->text);
echo get_class($token), ':', $token->pos;
"#);
    assert_eq!(out, "PhpToken:6");
}

/// An undefined constant in a subclass property default throws at instantiation, as PHP does.
#[test]
fn test_tokenizer_subclass_undefined_property_default_throws() {
    let out = compile_and_run(r#"<?php
class BrokenSourceToken extends PhpToken { public $extra = UNKNOWN; }
try { BrokenSourceToken::tokenize('<?php foo'); echo 'wrong'; }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Undefined constant \"UNKNOWN\"");
}

/// TOKEN_PARSE preserves a throwing user error handler for non-canonical casts.
#[test]
fn test_tokenizer_deprecation_handler_throw_propagates() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message): bool {
    throw new RuntimeException('handler: ' . $message);
});
try { PhpToken::tokenize('<?php (double) $x;', TOKEN_PARSE); echo 'wrong'; }
catch (RuntimeException $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "handler: Non-canonical cast (double) is deprecated, use the (float) cast instead");
}

/// A tokenizer-only program enforces the compiled heap capacity without `ini_set`.
#[test]
fn test_tokenizer_memory_limit_defaults_without_ini_set() {
    let out = compile_and_run_expect_failure(r#"<?php
$tokens = token_get_all(str_repeat('<?php echo 1;', 8000));
echo count($tokens);
"#);
    assert!(out.contains("Allowed memory size"), "{out}");
    assert!(out.contains("on line 2"), "{out}");
    assert!(!out.contains("tokenizer.php"), "{out}");
}

/// A namespaced user `ini_set` declaration must not be used by the memory guard.
#[test]
fn test_tokenizer_memory_limit_ignores_namespaced_ini_set() {
    let out = compile_and_run(r#"<?php
namespace Foo;
function ini_set(string $key, string $value): string { return $value; }
echo is_array(token_get_all('<?php echo 1;')) ? 'ok' : 'bad';
"#);
    assert_eq!(out, "ok");
}

/// Core INI quantity forms reach the memory guard as the numbers Zend parses.
///
/// The guard used to read the previous `memory_limit` with `(int)` plus its last character,
/// which turned `0x1000000` into zero and made every nonempty scan a fatal.
#[test]
fn test_tokenizer_memory_limit_accepts_core_ini_quantity_forms() {
    // The in-process fixture harness has no opcache prelude, so `ini_set` only exists on the
    // real CLI pipeline that also injects the quantity parser this guard shares.
    let out = compile_cli_file_and_run(r#"<?php
foreach (['0x1000000', '16M ', '1024K'] as $limit) {
    ini_set('memory_limit', $limit);
    echo is_array(token_get_all('<?php echo 1;')) ? 'ok' : 'bad';
}
"#, &[]);
    assert_eq!(out, "okokok");
}

/// The tokenizer allocation guard reports the caller's file and line, not a synthetic prelude.
#[test]
fn test_tokenizer_memory_limit_reports_call_site() {
    let out = compile_cli_file_expect_runtime_failure(r#"<?php
ini_set('memory_limit', '4M');
token_get_all('<?php ' . str_repeat('$a = 1; ', 30000));
"#);
    assert!(out.contains("Allowed memory size of 4194304 bytes exhausted"), "{out}");
    assert!(out.contains("on line 3"), "{out}");
    assert!(!out.contains("tokenizer.php"), "{out}");
}

/// Tokenizer diagnostics report the line inside the tokenized source, like PHP.
#[test]
fn test_tokenizer_warning_reports_tokenized_source_line() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo $level, ':', $line, "\n";
    return true;
});
$src = "<?php\n\n\n(integer)\$a;\n";
token_get_all($src, TOKEN_PARSE);
"#);
    assert_eq!(out, "8192:4\n");
}

/// Octal-escape overflow warns without TOKEN_PARSE and reports its own line.
#[test]
fn test_tokenizer_octal_overflow_reports_tokenized_source_line() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo $level, ':', $line, ':', $message, "\n";
    return true;
});
$src = "<?php\n\$s = \"\\400\";\n";
token_get_all($src);
"#);
    assert_eq!(
        out,
        "2:2:Octal escape sequence overflow \\400 is greater than \\377\n"
    );
}

/// PHP emits the diagnostics that precede a parse failure before throwing.
#[test]
fn test_tokenizer_warning_precedes_parse_error() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo $level, ':', $line, "\n";
    return true;
});
try {
    token_get_all("<?php\n\n(integer)\$a;\n\n\n +;", TOKEN_PARSE);
} catch (ParseError $e) {
    echo 'caught:', $e->getLine(), "\n";
}
"#);
    assert_eq!(out, "8192:3\ncaught:6\n");
}

/// Nowdoc bodies have no interpolation, so every indented line is stripped like PHP does.
#[test]
fn test_tokenizer_nowdoc_indentation_strips_unclosed_brace_lines() {
    let out = compile_and_run(r#"<?php
$nowdoc = <<<'A'
    {$a
    b}
    A;
var_export($nowdoc);
echo "\n";
$heredoc = <<<A
    \{$a
    b}
    A;
var_export($heredoc);
"#);
    assert_eq!(out, "'{$a\nb}'\n'\\\\{\nb}'");
}

/// Eval reports tokenizer warnings at the line inside the scanned source.
#[test]
fn test_tokenizer_eval_warning_reports_tokenized_source_line() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo $level, ':', $line, "\n";
    return true;
});
$code = $argc > 0 ? '
token_get_all("<?php\\n\\n(integer)\$a;\\n", TOKEN_PARSE);
' : '';
eval($code);
"#);
    assert_eq!(out, "8192:3\n");
}

/// `PhpToken::tokenize` keeps the diagnostics that precede a parse failure.
#[test]
fn test_tokenizer_eval_object_scan_reports_warnings_before_error() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo $level, ':', $line, "\n";
    return true;
});
$code = $argc > 0 ? '
try { PhpToken::tokenize("<?php\\n\\n(integer)\$a;\\n\\n\\n +;", TOKEN_PARSE); }
catch (ParseError $e) { echo "caught:", $e->getLine(), "\n"; }
' : '';
eval($code);
"#);
    assert_eq!(out, "8192:3\ncaught:6\n");
}

/// The tokenizer memory guard honors `--ini memory_limit` without an `ini_set` call.
#[test]
fn test_tokenizer_memory_guard_reads_configured_ini_limit() {
    let directory = make_cli_test_dir("tokenizer_memory_limit");
    let php = directory.join("main.php");
    std::fs::write(&php, r#"<?php
echo ini_get('memory_limit'), "\n";
$src = '<?php ' . str_repeat('$a = 1; ', 30000) . "\n";
token_get_all($src);
echo "ok\n";
"#).unwrap();
    let output = elephc_cli_command(&directory)
        .args(["--ini", "memory_limit=4M"])
        .arg(&php)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let executed = std::process::Command::new(php.with_extension("")).output().unwrap();
    let stdout = String::from_utf8_lossy(&executed.stdout);
    assert!(stdout.contains("4M"), "{stdout}");
    assert!(stdout.contains("Allowed memory size"), "{stdout}");
    let _ = std::fs::remove_dir_all(directory);
}

/// The memory guard honors `--ini memory_limit` even when the program never calls `ini_get`.
#[test]
fn test_tokenizer_memory_guard_reads_configured_ini_limit_without_ini_get() {
    let directory = make_cli_test_dir("tokenizer_memory_limit_no_ini_get");
    let php = directory.join("main.php");
    std::fs::write(&php, r#"<?php
$src = '<?php ' . str_repeat('$a = 1; ', 30000) . "\n";
token_get_all($src);
echo "ok\n";
"#).unwrap();
    let output = elephc_cli_command(&directory)
        .args(["--ini", "memory_limit=4M"])
        .arg(&php)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let executed = std::process::Command::new(php.with_extension("")).output().unwrap();
    let stdout = String::from_utf8_lossy(&executed.stdout);
    assert!(stdout.contains("Allowed memory size of 4194304 bytes exhausted"), "{stdout}");
    let _ = std::fs::remove_dir_all(directory);
}

/// Tokenizer warnings leave the handler's file empty, like php_error_docref(NULL, ...).
#[test]
fn test_tokenizer_warning_uses_empty_file_field() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo strlen($file), ':', $line, "\n";
    return true;
});
token_get_all("<?php\n\n(integer)\$a;\n", TOKEN_PARSE);
"#);
    assert_eq!(out, "0:3\n");
}

/// Eval tokenizer warnings leave the handler's file empty, like php_error_docref(NULL, ...).
#[test]
fn test_tokenizer_eval_warning_uses_empty_file_field() {
    let out = compile_and_run(r#"<?php
set_error_handler(function (int $level, string $message, string $file, int $line): bool {
    echo strlen($file), ':', $line, "\n";
    return true;
});
$code = $argc > 0 ? '
token_get_all("<?php\\n\\n(integer)\\$a;\\n", TOKEN_PARSE);
' : '';
eval($code);
"#);
    assert_eq!(out, "0:3\n");
}
