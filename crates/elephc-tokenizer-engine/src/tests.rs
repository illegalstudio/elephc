//! Purpose:
//! Pins tokenizer losslessness, the PHP symbol catalog and differential Zend parity.
//!
//! Called from:
//! - The tokenizer crate's CI test job, with PHP 8.5 as the reference interpreter.
//!
//! Key details:
//! - Differential results compare symbolic names, raw bytes, line numbers and positions.
//! - TOKEN_PARSE fixtures include PHP syntax beyond elephc's compiled subset.

use super::*;
#[cfg(feature = "parse")]
use std::io::Write;
#[cfg(feature = "parse")]
use std::process::{Command, Stdio};
#[cfg(feature = "parse")]
use serde_json::{json, Value};

/// Every tokenizer constant exists and the historical alias resolves canonically.
#[test]
fn tokenizer_catalog_has_all_154_php85_constants() {
    use elephc_builtin_contract::{constants, PhpModule};
    assert_eq!(constants().iter().filter(|c| c.module == PhpModule::Tokenizer).count(), 154);
    assert_eq!(id("T_DOUBLE_COLON"), id("T_PAAMAYIM_NEKUDOTAYIM"));
    assert_eq!(token_name(id("T_DOUBLE_COLON")), Some("T_DOUBLE_COLON"));
    assert_eq!(token_name(0), None);
    assert_eq!(token_name(i64::MAX), None);
}

/// Arbitrary source bytes survive tokenization with no gaps, overlaps or UTF-8 conversion.
#[test]
fn tokenizer_preserves_original_bytes_and_coordinates() {
    for source in fixtures() {
        let tokens = tokenize(&source, 0).unwrap();
        let mut pos = 0;
        let mut line = 1;
        for token in tokens {
            assert_eq!(token.pos, pos, "{}", hex(&source));
            assert_eq!(token.line, line, "{}", hex(&source));
            line += newline_count(&source[token.pos..token.pos + token.len]);
            pos += token.len;
        }
        assert_eq!(pos, source.len(), "{}", hex(&source));
    }
}

/// A lexer-only build rejects TOKEN_PARSE instead of silently skipping validation.
#[cfg(not(feature = "parse"))]
#[test]
fn tokenizer_parse_requires_the_parse_feature() {
    let error = tokenize(b"<?php function broken(", TOKEN_PARSE)
        .expect_err("lexer-only builds cannot validate TOKEN_PARSE");
    assert!(error.message.contains("'parse' feature"), "{}", error.message);
    assert!(tokenize(b"<?php function broken(", 0).is_ok());
}

/// TOKEN_PARSE accepts empty dimensions and semi-reserved `goto` names in PHP-valid positions.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php85_accepts_empty_dimensions_and_goto_names() {
    for source in [
        "<?php echo $a[];",
        "<?php $a[] ??= 1;",
        "<?php foo()[] = 1;",
        "<?php $a[]--;",
        "<?php $a[]++;",
        "<?php $a[]->b[];",
        "<?php X::goto;",
        "<?php foo(goto: 1);",
        "<?php class E { const goto = 1; }",
        "<?php $o->goto;",
        "<?php abstract class C { abstract public $a { get; } }",
        "<?php if (1) lab: echo 1;",
        "<?php { echo 1; }",
        "<?php if (1) { { echo 1; } }",
        "<?php foo()[0]++;",
        "<?php ++foo();",
        "<?php --foo();",
        "<?php $x = foo()++;",
        "<?php list() = [];",
        "<?php [] = [];",
        "<?php function f(A|(B&C) $x) {}",
        "<?php [$p, &$q] = $arr;",
        "<?php foreach ([1] as [$p, &$q]) {}",
        "<?php static fn($x) => $x;",
        "<?php static function () {};",
        "<?php #[Attr(1)] const X = 1;",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_ok(), "{source}");
    }
    let source = b"<?php X::goto; foo(goto: 1); class E { const goto = 1; }";
    let tokens = tokenize(source, TOKEN_PARSE).expect("semi-reserved goto names are valid PHP");
    for token in tokens.iter().filter(|token| &source[token.pos..token.pos + token.len] == b"goto") {
        assert_eq!(token.id, id("T_STRING"));
    }
}

/// TOKEN_PARSE accepts PHP 8.5 grammar that elephc's compiler subset rejects or lowers later.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php85_accepts_grammar_beyond_compiler_subset() {
    for source in [
        "<?php $x ? $a = 1 : 0;",
        "<?php new $this->_class();",
        "<?php new $a->b();",
        "<?php new $a['x']();",
        "<?php new A::$a();",
        "<?php new static::$a();",
        "<?php new $a::$b();",
        "<?php new (foo())();",
        "<?php new (foo());",
        "<?php if (list($a, $b) = explode(' ', $x)) {}",
        "<?php while ([$k, $v] = f()) {}",
        "<?php $x = [$a, $b] = [1, 2];",
        "<?php f([$a] = $b);",
        "<?php fn() => [$a] = $b;",
        "<?php @list($a, $b) = f();",
        "<?php foreach ($x as $a[]) {}",
        "<?php foreach ($x as $a->b) {}",
        "<?php foreach ($x as A::$b) {}",
        "<?php foreach ($x as f()) {}",
        "<?php function __construct(final public int $v = 1) {}",
        "<?php class C { public $x { get; set; } }",
        "<?php class C { public $x { final get => 1; } }",
        "<?php class C { public $x { #[A] get => 1; } }",
        "<?php class C { public static $x { get => 1; } }",
        "<?php class C { public $x = 1 { get => 1; } }",
        "<?php class C { public $x { foo => 1; } }",
        "<?php new #[A] class {};",
        "<?php new #[A] readonly class {};",
        "<?php #[A] fn() => 1;",
        "<?php #[A] function() {};",
        "<?php #[A] static fn() => 1;",
        "<?php #[A] const X = 1;",
        "<?php class C { #[A] case X; }",
        "<?php function f(#[A] public $x) {}",
        "<?php $d |> fn($x) => $x;",
        "<?php $b |> static fn($x) => $x;",
        "<?php $b |> include 'x';",
        "<?php new A()::X;",
        "<?php new A()::$x;",
        "<?php $a()::b;",
        "<?php $a[0]::b;",
        "<?php 'A'::b;",
        "<?php (A)::b;",
        "<?php f()::$b;",
        "<?php [A::class, 'b']();",
        "<?php X::class();",
        "<?php class E { const class = 7; }",
        "<?php enum E { case class; }",
        "<?php class C { function new() {} }",
        "<?php class C { function class() {} }",
        "<?php static fn() => 1;",
        "<?php static function() {};",
        "<?php [,];",
        "<?php [1,,2];",
        "<?php [1, 2] = $a;",
        "<?php [...$a] = $b;",
        "<?php list(1) = $a;",
        "<?php function f(int $x, ...$y, $z) {}",
        "<?php function f(public $x) {}",
        "<?php enum E { public $x; }",
        "<?php interface I { public $x; }",
        "<?php class C { static readonly $x; }",
        "<?php class C { var $x; }",
        "<?php class C { private(set) $x; }",
        "<?php class C { public private(set) $x; }",
        "<?php switch (1) { default: default: }",
        "<?php break 0;",
        "<?php break $a;",
        "<?php try {}",
        "<?php declare(foo=1+1);",
        "<?php namespace namespace;",
        "<?php for ($i = 0, $j = 0; $i < 1, $j < 2; $i++, $j++) {}",
        "<?php function f((A&B)|C $x) {}",
        "<?php function f(C|(A&B) $x) {}",
        "<?php function f(A &$x) {}",
        "<?php isset($a); empty($a); unset($a);",
        "<?php exit; exit(1); die('x');",
        "<?php class C { use T { T::c insteadof D; } }",
        "<?php use A\\{B, C}; use function A\\{b}; use const A\\{B};",
        "<?php if (1) { ?> html <?php } else { ?> other <?php }",
        "<?php if (1): ?> html <?php else: ?> other <?php endif;",
        "<?php __halt_compiler();",
        "<?php namespace X; __halt_compiler();",
        "<?php (double)$x; (unset)$x; (void)$x;",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_ok(), "{source}");
    }
}

/// TOKEN_PARSE rejects elephc dialect syntax and grammar-invalid constructs PHP rejects.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php85_rejects_dialect_and_accept_invalid_inputs() {
    for source in [
        "<?php extern 'System' { fn puts(s: ptr<u8>) -> i32; }",
        "<?php packed class P { public int $x; }",
        "<?php function f(array<int> $x) {}",
        "<?php final class B<T> {}",
        "<?php int $index = 1;",
        "<?php PDORow $typedLazy = $lazy;",
        "<?php $x = new Box<int>();",
        "<?php class C extends B<int> {}",
        "<?php $box->map<string>($f);",
        "<?php 1 + 2 = 3;",
        "<?php ($a) = 1;",
        "<?php ($a)++;",
        "<?php ++$a = 1;",
        "<?php $a = &new C;",
        "<?php $a = &1;",
        "<?php $a::class = 1;",
        "<?php $a == $b == $c;",
        "<?php $a < $b < $c;",
        "<?php $a <=> $b <=> $c;",
        "<?php $a <= $b >= $c;",
        "<?php isset();",
        "<?php empty();",
        "<?php unset();",
        "<?php exit->foo;",
        "<?php class C { $x; }",
        "<?php class C { int $x; }",
        "<?php class C { public (set) $x; }",
        "<?php function f(A&B|C $x) {}",
        "<?php function f(C|A&B $x) {}",
        "<?php function f(A&(B|C) $x) {}",
        "<?php try {} catch (A&B $e) {}",
        "<?php try {} catch (A $e);",
        "<?php try ; catch (A $e) {}",
        "<?php use A\\{};",
        "<?php use function A\\{function b};",
        "<?php function f() { use A; }",
        "<?php function f() { const A = 1; }",
        "<?php if (1) { const A = 1; }",
        "<?php class C { use T { c insteadof D; } }",
        "<?php if (1) ?> html <?php else ?> other <?php ;",
        "<?php if (1) __halt_compiler();",
        "<?php function f() { __halt_compiler(); }",
        "<?php function f() { $s = \"{$a}\"; __halt_compiler(); }",
        "<?php new $a::B();",
        "<?php new A::B();",
        "<?php new (foo())[0];",
        "<?php new (foo())->bar();",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_err(), "{source}");
    }
}

/// TOKEN_PARSE reports a catchable parser error while plain scanning accepts incomplete PHP.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_parse_validates_and_reclassifies_reserved_identifiers() {
    assert!(tokenize(b"<?php function broken(", 0).is_ok());
    assert!(tokenize(b"<?php function broken(", TOKEN_PARSE).is_err());
    let source = b"<?php class C { public const PUBLIC = 1; public function match() {} }";
    let raw = tokenize(source, 0).unwrap();
    let parsed = tokenize(source, TOKEN_PARSE).unwrap();
    assert!(raw.iter().zip(&parsed).any(|(a,b)| a.id != b.id && b.id == id("T_STRING")));
    assert_eq!(raw.iter().map(|t| (t.pos,t.len,t.line)).collect::<Vec<_>>(), parsed.iter().map(|t| (t.pos,t.len,t.line)).collect::<Vec<_>>());
}

/// TOKEN_PARSE diagnostic wording and lines match the PHP 8.5 oracle.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php85_parse_error_messages_match_php() {
    compare_messages_with_php85(&parse_message_corpus());
}

/// Returns the curated message-parity corpus.
#[cfg(feature = "parse")]
fn parse_message_corpus() -> Vec<Vec<u8>> {
    let mut sources: Vec<Vec<u8>> = [
        "<?php 0b;", "<?php 0b2;", "<?php 0x;", "<?php 0xg;", "<?php 09;",
        "<?php 1e;", "<?php 1e+;", "<?php 1..2;", "<?php .e1;", "<?php 1_;",
        "<?php 1__2;", "<?php 0o8;", "<?php 1.5.6;", "<?php 1f;", "<?php 1_000_;",
        "<?php 0o78;", "<?php 078;", "<?php 0x1g;", "<?php 0b12;",
        "<?php function broken(", "<?php function f(", "<?php function f($x",
        "<?php class C {", "<?php class C { public", "<?php if (", "<?php if (1",
        "<?php if (1) {", "<?php while (", "<?php for (;;", "<?php foreach ($a as",
        "<?php switch (1) { case", "<?php try {", "<?php catch (", "<?php echo",
        "<?php return", "<?php new", "<?php $a =", "<?php $a->", "<?php $a[",
        "<?php list(", "<?php array(", "<?php [1,", "<?php match (1) {",
        "<?php fn(", "<?php use", "<?php namespace", "<?php declare(",
        "<?php const", "<?php enum", "<?php interface", "<?php trait", "<?php #[",
        "<?php \"unterminated", "<?php 'unterminated", "<?php /* unterminated",
        "<?php `unterminated", "<?php goto;", "<?php goto 1;", "<?php goto $a;",
        "<?php goto label", "<?php goto label; label: echo 1;",
        "<?php 1 + 2 = 3;", "<?php ($a) = 1;", "<?php array &$arr;",
        "<?php new (foo())[0];", "<?php function f(A&B|C $x) {}",
        "<?php $a == $b == $c;", "<?php isset();", "<?php empty();",
        "<?php unset();", "<?php exit->foo;", "<?php class C { $x; }",
        "<?php class C { public (set) $x; }", "<?php try {} catch (A $e);",
        "<?php use A\\{};", "<?php if (1) ?> html <?php else ?> other <?php ;",
        "<?php if (1) __halt_compiler();", "<?php new $a::B();",
        "<?php new A::B();",
        "<?php\nfunction broken(", "<?php\nclass C {", "<?php\nif (1) {",
        "<?php\n$a[", "<?php\n0b;", "<?php\n1..2;", "<?php\ngoto;",
        "<?php\nreturn",
    ]
    .into_iter()
    .map(|source| source.as_bytes().to_vec())
    .collect();
    sources.push(b"<?php \"$a[\\1]\";".to_vec());
    sources.push(b"<?php \"$a[\x00]\";".to_vec());
    sources.push(b"<?php \"$a[1\x00]\";".to_vec());
    sources
}

/// Compares TOKEN_PARSE messages and lines for every source against the PHP 8.5 oracle.
#[cfg(feature = "parse")]
fn compare_messages_with_php85(sources: &[Vec<u8>]) {
    let available = Command::new("php")
        .args(["-r", "echo PHP_VERSION_ID >= 80500 && PHP_VERSION_ID < 80600 && extension_loaded('tokenizer') ? 'yes' : 'no';"])
        .output();
    if !available.is_ok_and(|output| output.stdout == b"yes") {
        assert!(
            std::env::var_os("ELEPHC_TOKENIZER_REQUIRE_PHP85").is_none(),
            "CI requires PHP 8.5 with tokenizer enabled"
        );
        return;
    }
    let input = serde_json::to_vec(&sources.iter().map(|source| hex(source)).collect::<Vec<_>>()).unwrap();
    let script = r#"
        $out = [];
        foreach (json_decode(stream_get_contents(STDIN), true) as $hex) {
            $code = hex2bin($hex);
            try {
                PhpToken::tokenize($code, TOKEN_PARSE);
                $out[] = null;
            } catch (Throwable $e) {
                $out[] = [$e->getMessage(), $e->getLine()];
            }
        }
        echo json_encode($out, JSON_THROW_ON_ERROR);
    "#;
    let mut child = Command::new("php")
        .args(["-d", "short_open_tag=0", "-d", "error_reporting=0", "-r", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "PHP message oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut mismatches = Vec::new();
    for (source, expected) in sources.iter().zip(expected) {
        let got = match tokenize_with_warnings(source, TOKEN_PARSE) {
            Ok(_) => Value::Null,
            Err(error) => json!([error.message, error.line]),
        };
        if got != expected {
            mismatches.push(format!(
                "source={}\n  got {got}\n  expected {expected}",
                String::from_utf8_lossy(source)
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
}

/// Echo statement validation must not reject a reserved word used as a constant name.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_parse_distinguishes_echo_statements_from_constant_names() {
    let source = b"<?php class C { const ECHO = 1; } C::ECHO; echo C::ECHO;";
    let parsed = tokenize(source, TOKEN_PARSE).expect("reserved constant names are valid PHP");
    let names: Vec<_> = parsed.iter().filter(|t| &source[t.pos..t.pos + t.len] == b"ECHO").collect();
    assert_eq!(names.len(), 3);
    assert!(names.iter().all(|t| t.id == id("T_STRING")));
    assert!(parsed.iter().any(|t| t.id == id("T_ECHO")));
    for invalid in ["<?php echo;", "<?php echo /*comment*/ ;", "<?php echo 1,;", "<?=?>", "<?= ; ?>"] {
        assert!(tokenize(invalid.as_bytes(), TOKEN_PARSE).is_err(), "{invalid}");
    }
}

/// Zend excludes extends/implements prefixes from enum declaration lookahead.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_enum_lookahead_preserves_php85_prefix_exclusions() {
    for name in ["extendsFoo", "implementsFoo", "ExTeNdS_Foo", "ImPlEmEnTs_Foo", "extendsé"] {
        let source = format!("<?php enum {name} {{ case A; }}");
        let tokens = tokenize(source.as_bytes(), 0).unwrap();
        assert_eq!(tokens.iter().find(|t| t.pos == 6).unwrap().id, id("T_STRING"), "{source}");
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_err(), "{source}");
    }
    for name in ["FooExtends", "FooImplements", "XextendsFoo", "XimplementsFoo"] {
        let source = format!("<?php enum {name} {{ case A; }}");
        for flags in [0, TOKEN_PARSE] {
            let tokens = tokenize(source.as_bytes(), flags).unwrap();
            assert_eq!(tokens.iter().find(|t| t.pos == 6).unwrap().id, id("T_ENUM"), "{source}, flags={flags}");
        }
    }
}

/// Compares the same corpus against PHP 8.5's Zend tokenizer in both flag modes.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php85_differential_corpus() {
    compare_with_php85(&fixtures());
}

/// Compares every php-src ext/tokenizer test file against the PHP 8.5 oracle.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php_src_ext_tokenizer_files_match_php() {
    compare_with_php85(&php_src_fixtures());
}

/// Loads the vendored `--FILE--` sources from php-src's ext/tokenizer/tests suite.
#[cfg(feature = "parse")]
fn php_src_fixtures() -> Vec<Vec<u8>> {
    const FILES: &str = include_str!("../tests/data/php_src_tokenizer_files.json");
    serde_json::from_str::<Vec<String>>(FILES)
        .expect("vendored php-src tokenizer fixture must be a JSON string list")
        .into_iter()
        .map(|source| (0..source.len()).step_by(2)
            .map(|index| u8::from_str_radix(&source[index..index + 2], 16).expect("hex fixture byte"))
            .collect())
        .collect()
}

/// Compares tokenization and TOKEN_PARSE outcomes for every source against PHP 8.5.
#[cfg(feature = "parse")]
fn compare_with_php85(sources: &[Vec<u8>]) {
    let available = Command::new("php").args(["-r", "echo PHP_VERSION_ID >= 80500 && PHP_VERSION_ID < 80600 && extension_loaded('tokenizer') ? 'yes' : 'no';"]).output();
    if !available.is_ok_and(|output| output.stdout == b"yes") {
        assert!(std::env::var_os("ELEPHC_TOKENIZER_REQUIRE_PHP85").is_none(), "CI requires PHP 8.5 with tokenizer enabled");
        return;
    }
    let input = serde_json::to_vec(&sources.iter().map(|s| hex(s)).collect::<Vec<_>>()).unwrap();
    let script = r#"
        $out = [];
        foreach (json_decode(stream_get_contents(STDIN), true) as $hex) {
            $code = hex2bin($hex);
            foreach ([0, TOKEN_PARSE] as $flags) {
                try {
                    $row = []; $pos = 0; $line = 1;
                    foreach (PhpToken::tokenize($code, $flags) as $t) {
                        $row[] = [$t->id < 256 ? null : token_name($t->id), bin2hex($t->text), $t->line, $t->pos];
                    }
                    $out[] = $row;
                } catch (Throwable $e) { $out[] = 'ParseError'; }
            }
        }
        echo json_encode($out, JSON_THROW_ON_ERROR);
    "#;
    let mut child = Command::new("php").args(["-d", "short_open_tag=0", "-d", "error_reporting=0", "-r", script])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "PHP oracle failed: {}", String::from_utf8_lossy(&output.stderr));
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut case = 0;
    let mut mismatches = Vec::new();
    for source in sources {
        for flags in [0, TOKEN_PARSE] {
            let got = match tokenize(source, flags) {
                Ok(tokens) => Value::Array(tokens.iter().map(|t| json!([
                    if t.id < 256 { None } else { token_name(t.id) },
                    hex(&source[t.pos..t.pos + t.len]), t.line, t.pos,
                ])).collect()),
                Err(_) => json!("ParseError"),
            };
            if got != expected[case] {
                let detail = match (&got, &expected[case]) {
                    (Value::Array(a), Value::Array(b)) => {
                        let index = a.iter().zip(b).position(|(a,b)| a != b).unwrap_or(a.len().min(b.len()));
                        format!("token {index}: got {:?}; expected {:?}; counts {} vs {}", a.get(index), b.get(index), a.len(), b.len())
                    }
                    _ => format!("got {}; expected {}", if got.is_array() { "tokens" } else { "ParseError" }, if expected[case].is_array() { "tokens" } else { "ParseError" }),
                };
                mismatches.push(format!("flags={flags}, source={}\n{detail}", String::from_utf8_lossy(source)));
            }
            case += 1;
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
}
/// Covers trivia, PHP 8.5 syntax, parser feedback, interpolation and malformed input.
fn fixtures() -> Vec<Vec<u8>> {
    let mut sources = vec![
        "", "plain HTML", "<?php", "<?phpx echo 1;", "<?PHP\r\necho 1; ?>\r\nhtml",
        "before<?=1?>after", "<?xml version='1.0'?>", "<? echo 1; ?>", "#!/usr/bin/php\n<?php echo 1;",
        "<?php \t\r\n\r\necho 1; // comment\n# comment ?>html",
        "<?php /** doc */ /**not_doc*/ /**/ /****/ /* comment */ echo 1;",
        "<?php class C { public const PUBLIC = 1; public function match() {} }",
        "<?php class C { const ECHO = 1; } C::ECHO; echo C::ECHO;",
        "<?php class C { const ECHO = 1; } C::ECHO /*comment*/ ; ?>",
        "<?php $x->class; $x?->match; $x-> /*comment*/ function; C::class; C::match();",
        "<?php namespace A\\B; use Foo\\Bar as B; \\strlen('x'); namespace\\x();",
        "<?php (int\n)$x; (i n t)$x; ( int )$x; (int\r\n);",
        "<?php $x->foo\\bar; $x?->namespace\\name;",
        r#"<?php $s = "\u{D800}"; $s = "\\u{110000}"; $s = '\u{110000}';"#,
        "<?php $s = <<<TXT\n\\u{110000}\nTXT;",
        "<?php $s = <<<'TXT'\n\\u{110000}\nTXT;",
        r#"<?php $s = "\u{FFFFFFFFFFFFFFFFFFFFFF}";"#,
        r#"<?php $s = "\u{110000}";"#, r#"<?php $s = "\u{}";"#,
        r#"<?php $s = "\u{unterminated";"#,
        "<?php class C { public function class() {} public function clone() {} } C::class; clone($x);",
        "<?php (real)$x;", "<?php (unset)$x;", "<?php (void)$x;", "<?=?>",
        "<?php echo 1,;", "<?php print;", "<?php throw;", "<?php yield from;",
        "<?php __halt_compiler( /*c*/ ) /*c*/ ; raw", "<?php __halt_compiler // comment\n ( ) ; raw",
        "<?php function & /*comment*/ f(& /*comment*/ $x, &...$rest) { return $x; } $a & $b; $a & 1;",
        "<?php function f(){yield /*comment*/ from []; yield\nfrom []; yield fromx;}",
        "<?php enum A: string { case B = 'b'; } function enum() {} enum(); enum extends;",
        "<?php enum extendsFoo { case A; } enum implementsFoo { case B; }",
        "<?php enum /*comment*/ ExTeNdS_Foo { case A; } enum ImPlEmEnTs_Foo { case B; }",
        "<?php enum extendsé { case A; }", "<?php enum extends;", "<?php enum implements;",
        "<?php enum FooExtends { case A; } enum FooImplements { case B; }",
        "<?php enum XextendsFoo { case A; } enum XimplementsFoo { case B; }",
        "<?php function enum() {} enum();",
        "<?php $x = (int)1; (integer)$x; (bool)$x; (boolean)$x; (binary)$x; (double)$x; (real)$x; (float)$x; (array)$x; (object)$x; (unset)$x; (void)$x;",
        "<?php 0; 00; 0777; 0o77; 0xF; 0b11; 1_000; 1.5; .2; 2.; 1e3; 1E-2; 9223372036854775808; 0xffffffffffffffff;",
        "<?php echo __CLASS__, __TRAIT__, __METHOD__, __FUNCTION__, __PROPERTY__, __LINE__, __FILE__, __DIR__, __NAMESPACE__;",
        "<?php $s = 'single \\' quote'; $s = b'bytes'; $s = B\"bytes\"; $s = \"plain\";",
        "<?php $s = \"hello $name {$obj->class} ${name} $a[0] $a[foo] $a[$i]\";",
        "<?php $s = \"{$a['key']} {$a->{foo()}} ${foo[1]}\";",
        "<?php $s = `echo $name`;",
        "<?php $s = <<<TXT\nhello $name\nTXT;\necho $s;",
        "<?php $s = <<<'TXT'\r\nhello $name\r\nTXT;\r\necho $s;",
        "<?php $s = <<<TXT\n    hello {$a->b}\n    TXT;",
        "<?php #[Attribute] class A { public private(set) string $x { get => $this->x; } }",
        "<?php $x = 1 |> strval(...); clone($x, []);",
        "<?php __halt_compiler();RAW<?php data", "<?php __halt_compiler /*c*/ ( ) ;\nRAW",
        "<?php __halt_compiler a b c RAW", "<?php function broken(", "<?php echo ;",
        "<?php 'unfinished", "<?php \"unfinished", "<?php /*unfinished", "<?php /** unfinished",
        "<?php 09; 0x; 0b; 1e;", "<?php }", "<?php $a = [1,;", "<?php \"{$a[\"b\"]}\";",
    ].into_iter().map(|s| s.as_bytes().to_vec()).collect::<Vec<_>>();
    sources.push(b"<?php // a\r\nb;".to_vec());
    sources.push(b"<?php # a\rb;".to_vec());
    sources.push(b"<?php $s = <<<TXT\nabc\\\nTXT;\necho 'ok';".to_vec());
    sources.push(b"<?php $s = <<<TXT\rabc\rTXT;\r".to_vec());
    sources.push(b"<?php <<<TXT".to_vec());
    sources.push(b"<?php <<<TXT foo\nTXT;".to_vec());
    sources.push(b"<?php <<<0TXT\nbody\n0TXT;".to_vec());
    sources.push(b"<?php <<<TXT\nbody\nTXT".to_vec());
    sources.push(b"<?php b<<<TXT\nbody\nTXT;".to_vec());
    sources.push(b"<?php b<<<'TXT'\nbody\nTXT;".to_vec());
    sources.push(b"<?php $s = \"$a[-1]\";".to_vec());
    sources.push(b"<?php $s = \"$a[1.5]\";".to_vec());
    sources.push(b"<?php $s = \"$a[\"b\"]\";".to_vec());
    sources.push(b"<?php $s = \"$a['x']\";".to_vec());
    sources.push(b"<?php $s = \"$a[]\";".to_vec());
    sources.push(b"<?php $s = \"$a?->b\";".to_vec());
    sources.push(b"<?php $s = \"$a[foo->bar]\";".to_vec());
    sources.push(b"<?php $s = \"$a[0x1]\";".to_vec());
    sources.push(b"<?php $s = \"{$f(#[A] fn() => 1)}\";".to_vec());
    sources.push(b"<?php public (set) $x;".to_vec());
    sources.push(b"<?php private(set\t) $x;".to_vec());
    sources.push(b"<?php foo(&$a);".to_vec());
    sources.push(b"<?php $s = `echo {$a +}`;".to_vec());
    sources.push(b"<?php goto a; a: echo 1;".to_vec());
    sources.push(b"<?php $$y = 1;".to_vec());
    sources.push(b"<?php echo ${$b};".to_vec());
    sources.push(b"<?php static $s = 1, $t;".to_vec());
    sources.push(b"<?php A::{'B'}::c();".to_vec());
    sources.push(b"<?php echo $a[]; $a[] ??= 1; foo()[] = 1; $a[]--; $a[]++; $a[]->b[];".to_vec());
    sources.push(b"<?php X::goto; foo(goto: 1); class E { const goto = 1; } $o->goto;".to_vec());
    sources.push(b"<?php abstract class C { abstract public $a { get; } } if (1) lab: echo 1;".to_vec());
    sources.push(b"<?php { echo 1; } foo()[0]++; ++foo(); --foo(); $x = foo()++;".to_vec());
    sources.push(b"<?php list() = []; [] = []; function f(A|(B&C) $x) {} [$p, &$q] = $arr;".to_vec());
    sources.push(b"<?php static fn($x) => $x; static function () {}; #[Attr(1)] const X = 1;".to_vec());
    sources.push(b"<?php new ($cls)();".to_vec());
    sources.push(b"<?php $a?->b::C;".to_vec());
    sources.push(b"<?php $a::$b::$c;".to_vec());
    sources.push(b"<?php A::B::C;".to_vec());
    sources.push(b"<?php $a->b::C;".to_vec());
    sources.push(b"<?php function f((A&B)|null $x) {}".to_vec());
    sources.push(b"<?php class C { public abstract $y { get; } }".to_vec());
    sources.push(b"<?php $f(...);".to_vec());
    sources.push(b"<?php echo \"\xff\"; echo ;".to_vec());
    sources.push(b"<?php \x0b\x0c\t echo 1;".to_vec());
    sources.push(b"<?php \0\x01\x7f; $\xff = '\xfe\0'; ?>\xff".to_vec());
    sources.push("<?php $città = 'è'; echo $città;".as_bytes().to_vec());

    // Four-model review corpus: PHP 8.5 grammar TOKEN_PARSE accepts even when its compiler
    // rejects the construct later, plus the elephc-dialect and accept-invalid negatives.
    for source in [
        "<?php $x ? $a = 1 : 0;",
        "<?php $x ? $a = 1 : $b = 2;",
        "<?php new $this->_class();",
        "<?php new $a->b();",
        "<?php new $a['x']();",
        "<?php new $a[0]();",
        "<?php new A::$a();",
        "<?php new static::$a();",
        "<?php new $a::B();",
        "<?php new (foo())[0];",
        "<?php if (list($a, $b) = explode(' ', $x)) {}",
        "<?php while ([$k, $v] = f()) {}",
        "<?php $x = [$a, $b] = [1, 2];",
        "<?php f([$a] = $b);",
        "<?php fn() => [$a] = $b;",
        "<?php @list($a, $b) = f();",
        "<?php @[$a, $b] = f();",
        "<?php foreach ($x as $a[]) {}",
        "<?php foreach ($x as $a->b) {}",
        "<?php foreach ($x as $a[0]) {}",
        "<?php foreach ($x as A::$b) {}",
        "<?php foreach ($x as f()) {}",
        "<?php function __construct(final public int $v = 1) {}",
        "<?php interface I { public $x { get; set; } }",
        "<?php class C { public $x { get; set; } }",
        "<?php class C { public $x { final get => 1; } }",
        "<?php class C { public $x { #[A] get => 1; } }",
        "<?php class C { public static $x { get => 1; } }",
        "<?php class C { public $x = 1 { get => 1; } }",
        "<?php class C { public $x { foo => 1; } }",
        "<?php new #[A] class {};",
        "<?php new #[A] readonly class {};",
        "<?php #[A] fn() => 1;",
        "<?php #[A] function() {};",
        "<?php #[A] static fn() => 1;",
        "<?php #[A] const X = 1;",
        "<?php class C { #[A] case X; }",
        "<?php function f(#[A] public $x) {}",
        "<?php $d |> fn($x) => $x;",
        "<?php $b |> static fn($x) => $x;",
        "<?php $b |> include 'x';",
        "<?php new A()::X;",
        "<?php new A()::$x;",
        "<?php $a()::b;",
        "<?php $a[0]::b;",
        "<?php \"A\"::b;",
        "<?php (A)::b;",
        "<?php f()::$b;",
        "<?php [A::class, \"b\"]();",
        "<?php X::class();",
        "<?php class E { const class = 7; }",
        "<?php enum E { case class; }",
        "<?php class C { function new() {} }",
        "<?php class C { function class() {} }",
        "<?php static fn() => 1;",
        "<?php static function() {};",
        "<?php [,];",
        "<?php [1,,2];",
        "<?php [1, 2,,3];",
        "<?php [1, 2] = $a;",
        "<?php [...$a] = $b;",
        "<?php list(1) = $a;",
        "<?php function f(int $x, ...$y, $z) {}",
        "<?php function f(public $x) {}",
        "<?php enum E { public $x; }",
        "<?php interface I { public $x; }",
        "<?php class C { static readonly $x; }",
        "<?php class C { var $x; }",
        "<?php class C { private(set) $x; }",
        "<?php class C { public private(set) $x; }",
        "<?php switch (1) { default: default: }",
        "<?php break 0;",
        "<?php break $a;",
        "<?php try {}",
        "<?php declare(foo=1+1);",
        "<?php namespace namespace;",
        "<?php for ($i = 0, $j = 0; $i < 1, $j < 2; $i++, $j++) {}",
        "<?php function f(A|(B&C) $x) {}",
        "<?php function f((A&B)|C $x) {}",
        "<?php function f(A&(B|C) $x) {}",
        "<?php function f(C|(A&B) $x) {}",
        "<?php function f(A &$x) {}",
        "<?php isset($a); empty($a); unset($a);",
        "<?php exit; exit(1); die('x');",
        "<?php class C { use T { T::c insteadof D; } }",
        "<?php use A\\{B, C}; use function A\\{b}; use const A\\{B};",
        "<?php if (1) { ?> html <?php } else { ?> other <?php }",
        "<?php if (1): ?> html <?php else: ?> other <?php endif;",
        "<?php __halt_compiler();",
        "<?php namespace X; __halt_compiler();",
        "<?php (double)$x; (unset)$x; (void)$x;",
        // PHP rejects these in TOKEN_PARSE; the engine must reject them too.
        "<?php 1 + 2 = 3;",
        "<?php ($a) = 1;",
        "<?php ($a)++;",
        "<?php ++$a = 1;",
        "<?php $a = &new C;",
        "<?php $a = &1;",
        "<?php $a::class = 1;",
        "<?php $a == $b == $c;",
        "<?php $a < $b < $c;",
        "<?php $a <=> $b <=> $c;",
        "<?php $a <= $b >= $c;",
        "<?php isset();",
        "<?php empty();",
        "<?php unset();",
        "<?php exit->foo;",
        "<?php class C { $x; }",
        "<?php class C { int $x; }",
        "<?php class C { public (set) $x; }",
        "<?php function f(A&B|C $x) {}",
        "<?php function f(C|A&B $x) {}",
        "<?php try {} catch (A&B $e) {}",
        "<?php try {} catch (A $e);",
        "<?php try ; catch (A $e) {}",
        "<?php use A\\{};",
        "<?php use function A\\{function b};",
        "<?php function f() { use A; }",
        "<?php function f() { const A = 1; }",
        "<?php if (1) { const A = 1; }",
        "<?php class C { use T { c insteadof D; } }",
        "<?php if (1) ?> html <?php else ?> other <?php ;",
        "<?php if (1) __halt_compiler();",
        "<?php function f() { __halt_compiler(); }",
        "<?php extern \"System\" { fn puts(s: ptr<u8>) -> i32; }",
        "<?php packed class P { public int $x; }",
        "<?php function f(array<int> $x) {}",
        "<?php final class B<T> {}",
        "<?php int $index = 1;",
        "<?php PDORow $typedLazy = $lazy;",
        "<?php $x = new Box<int>();",
        "<?php class C extends B<int> {}",
        "<?php $box->map<string>($f);",
    ] {
        sources.push(source.as_bytes().to_vec());
    }
    // Lexer boundary: Zend stops the T_NUM_STRING offset at the numeric prefix.
    for source in [
        "<?php \"$a[1e3]\";",
        "<?php \"$a[1abc]\";",
        "<?php \"$a[0b2]\";",
        "<?php \"$a[0xg]\";",
        "<?php \"$a[1_]\";",
        "<?php \"$a[1.5e3]\";",
        "<?php \"$a[1_000]\";",
        "<?php \"$a[0x1A]\";",
        "<?php \"$a[0b10]\";",
        "<?php \"$a[0o17]\";",
        "<?php \"$a[12abc]\";",
    ] {
        sources.push(source.as_bytes().to_vec());
    }
    // ST_VAR_OFFSET fallback: an unexpected byte emits an empty T_ENCAPSED_AND_WHITESPACE
    // and resumes the normal string body; control bytes are T_BAD_CHARACTER.
    sources.push(b"<?php \"$a[\\1]\";".to_vec());
    sources.push(b"<?php \"$a[1\n]\";".to_vec());
    sources.push(b"<?php \"$a[\\0]\";".to_vec());
    sources.push(b"<?php \"$a[\x00]\";".to_vec());
    sources.push(b"<?php \"$a[\x01]\";".to_vec());
    sources.push(b"<?php \"$a[1\x00]\";".to_vec());
    sources.push(b"<?php \"$a[\\1]x\";".to_vec());
    sources.push(b"<?php \"$a[ ]\";".to_vec());
    // Heredoc indentation inspects only the first `indent.len()` bytes of each body line.
    for source in [
        b"<?php $s = <<<EOT\n  abc\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n  \tabc\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n\t  abc\n\tEOT;".as_slice(),
        b"<?php $s = <<<EOT\n\t\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n abc\n\tEOT;".as_slice(),
        b"<?php $s = <<<EOT\n abc\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n   \n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n \n  EOT;".as_slice(),
    ] {
        sources.push(source.to_vec());
    }
    sources
}

/// Encodes source bytes for an ASCII-only cross-process oracle protocol.
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }

/// php-src tokenizer cases: every half-reserved name and trait adaptation reclassifies.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php_src_reserved_name_feedback() {
    let source = b"<?php\nX::continue;\nX::$continue;\n$x->$continue;\nX::continue();\n$x->continue();\nX::class;\n$x->__halt_compiler();\nclass X {\n    const CONTINUE = 1;\n    const ARRAY = [1, 2];\n}\nclass C {\n    use A {\n        namespace as bar;\n    }\n}\n";
    let tokens = tokenize(source, TOKEN_PARSE).expect("php-src semi-reserved source is valid PHP");
    for name in [b"continue".as_slice(), b"class", b"__halt_compiler", b"CONTINUE", b"ARRAY", b"namespace", b"bar"] {
        let token = tokens.iter().find(|token| &source[token.pos..token.pos + token.len] == name)
            .unwrap_or_else(|| panic!("missing token {}", String::from_utf8_lossy(name)));
        assert_eq!(token.id, id("T_STRING"), "{}", String::from_utf8_lossy(name));
    }
}

/// php-src tokenizer heredoc cases keep PHP's exact TOKEN_PARSE diagnostics and lines.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php_src_heredoc_parse_errors() {
    for (source, message, line) in [
        (b"<?php <<<INNER_END\na".as_slice(), "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\"", 2),
        (b"<?php <<<INNER_END\nab".as_slice(), "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\"", 2),
        (b"<?php\n \t<<<'DOC'\n \tXXX\n \tDOC;".as_slice(), "Invalid indentation - tabs and spaces cannot be mixed", 3),
        (b"<?php <<<INNER_END\nabc\n   INNER_END;".as_slice(), "Invalid body indentation level (expecting an indentation level of at least 3)", 2),
        (b"<?php <<<INNER_END\nab\nab\n\t".as_slice(), "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\"", 4),
    ] {
        let error = tokenize(source, TOKEN_PARSE).expect_err("malformed heredoc must fail with TOKEN_PARSE");
        assert_eq!(error.message, message, "{}", String::from_utf8_lossy(source));
        assert_eq!(error.line, line, "{}", String::from_utf8_lossy(source));
    }
    assert!(tokenize(b"<?php <<<INNER_END\n\n   INNER_END;", TOKEN_PARSE).is_ok());
}

/// php-src tokenizer invalid escapes report PHP's parser-facing messages.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php_src_unicode_escape_errors() {
    for (source, message) in [
        (r#"<?php var_dump("\u{xyz}");"#, "Invalid UTF-8 codepoint escape sequence"),
        (r#"<?php var_dump("\u{ffffff}");"#, "Invalid UTF-8 codepoint escape sequence: Codepoint too large"),
        (r#"<?php var_dump("\u{110000}");"#, "Invalid UTF-8 codepoint escape sequence: Codepoint too large"),
    ] {
        let error = tokenize(source.as_bytes(), TOKEN_PARSE).expect_err("invalid escape must fail");
        assert_eq!(error.message, message);
    }
}

/// Non-canonical casts surface PHP's deprecations only under TOKEN_PARSE.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php_src_non_canonical_cast_warnings() {
    let output = tokenize_with_warnings(b"<?php (double) $x; (integer) $x; (boolean) $x; (binary) $x;", TOKEN_PARSE)
        .expect("non-canonical casts are valid syntax");
    assert_eq!(output.warnings.len(), 4);
    assert_eq!(output.warnings[0].message, "Non-canonical cast (double) is deprecated, use the (float) cast instead");
    assert_eq!(output.warnings[1].message, "Non-canonical cast (integer) is deprecated, use the (int) cast instead");
    assert_eq!(output.warnings[2].message, "Non-canonical cast (boolean) is deprecated, use the (bool) cast instead");
    assert_eq!(output.warnings[3].message, "Non-canonical cast (binary) is deprecated, use the (string) cast instead");
    assert!(tokenize_with_warnings(b"<?php (double) $x;", 0).expect("plain scanning succeeds").warnings.is_empty());
}

/// `declare(encoding=...)` warns exactly as Zend does when multibyte support is off.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_declare_encoding_warning() {
    let output = tokenize_with_warnings(b"<?php declare(encoding='ISO-8859-1');", TOKEN_PARSE)
        .expect("declare(encoding) is valid syntax");
    assert_eq!(output.warnings.len(), 1);
    assert_eq!(
        output.warnings[0].message,
        "declare(encoding=...) ignored because Zend multibyte feature is turned off by settings"
    );
    assert!(tokenize_with_warnings(b"<?php declare(strict_types=1);", TOKEN_PARSE)
        .expect("strict_types is valid syntax")
        .warnings
        .is_empty());
}

/// php-src halt-compiler headers keep PHP's TOKEN_PARSE diagnostics.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php_src_halt_compiler_parse_errors() {
    for (source, message) in [
        (b"<?php __halt_compiler".as_slice(), "syntax error, unexpected end of file, expecting \"(\""),
        (b"<?php __halt_compiler(".as_slice(), "Unclosed '('"),
        (b"<?php __halt_compiler a b c RAW".as_slice(), "syntax error, unexpected identifier \"a\", expecting \"(\""),
        (b"<?php __halt_compiler( )".as_slice(), "syntax error, unexpected end of file, expecting \";\""),
    ] {
        let error = tokenize(source, TOKEN_PARSE).expect_err("malformed halt header must fail");
        assert_eq!(error.message, message, "{}", String::from_utf8_lossy(source));
    }
    assert!(tokenize(b"<?php __halt_compiler();RAW", TOKEN_PARSE).is_ok());
    assert!(tokenize(b"<?php __halt_compiler", 0).is_ok());
}


/// PHP reserves `list` in declaration and expression positions; semi-reserved uses stay valid.
#[cfg(feature = "parse")]
#[test]
fn tokenizer_php_src_reserved_list_positions() {
    for source in [
        "<?php function list() {}",
        "<?php class list {}",
        "<?php use A as list;",
        "<?php echo list;",
        "<?php print list;",
        "<?php $x = list;",
        "<?php $x instanceof list;",
        "<?php new list;",
        "<?php new list();",
        "<?php return list;",
        "<?php yield list;",
        "<?php fn() => list;",
        "<?php public list $x;",
        "<?php #[list] function f() {}",
        "<?php function f(): list {}",
        "<?php function f(list $x) {}",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_err(), "{source}");
    }
    for source in [
        "<?php class C { function list() {} }",
        "<?php class C { const LIST = 1; }",
        "<?php class C { use A { list as x; } }",
        "<?php foo(list: 1); function foo($x) {}",
        "<?php list($a) = [1];",
        "<?php X::list;",
        "<?php $o->list;",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_ok(), "{source}");
    }
}
