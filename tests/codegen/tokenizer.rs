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
