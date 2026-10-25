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
use std::io::Write;
use std::process::{Command, Stdio};
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

/// TOKEN_PARSE reports a catchable parser error while plain scanning accepts incomplete PHP.
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

/// Echo statement validation must not reject a reserved word used as a constant name.
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
#[test]
fn tokenizer_php85_differential_corpus() {
    let available = Command::new("php").args(["-r", "echo PHP_VERSION_ID >= 80500 && PHP_VERSION_ID < 80600 && extension_loaded('tokenizer') ? 'yes' : 'no';"]).output();
    if !available.is_ok_and(|output| output.stdout == b"yes") {
        assert!(std::env::var_os("ELEPHC_TOKENIZER_REQUIRE_PHP85").is_none(), "CI requires PHP 8.5 with tokenizer enabled");
        return;
    }
    let sources = fixtures();
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
                } catch (ParseError $e) { $out[] = 'ParseError'; }
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
            let got = match tokenize(&source, flags) {
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
                mismatches.push(format!("flags={flags}, source={}\n{detail}", String::from_utf8_lossy(&source)));
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
    sources.push(b"<?php \x0b\x0c\t echo 1;".to_vec());
    sources.push(b"<?php \0\x01\x7f; $\xff = '\xfe\0'; ?>\xff".to_vec());
    sources.push("<?php $città = 'è'; echo $città;".as_bytes().to_vec());
    sources
}

/// Encodes source bytes for an ASCII-only cross-process oracle protocol.
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }
