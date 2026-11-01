//! Purpose:
//! Parser tests for the two constructs the interpreter's scanner used to refuse outright:
//! a heredoc whose body interpolates, and an identifier holding a byte at or above `0x80`.
//!
//! Called from:
//! - `cargo test -p elephc-magician parser::tests` through Rust's test harness.
//!
//! Key details:
//! - A sweep of the Symfony vendor tree found fourteen files refused as
//!   `UnsupportedConstruct` for the heredoc shape and one (`symfony/cache` ValueWrapper) for
//!   the identifier shape, all of which `php -n -l` accepts.
//! - These cases assert the EvalIR a fragment lowers to, so a scanner that tokenizes the
//!   shape but drops a part still fails here.
//! - Every expected value is `php -n` 8.5.6 on the same fragment.

use super::support::*;

/// Builds the fragment byte string for a `$name` heredoc reducer.
///
/// `$a = "X"; $s = <<<EOT\n    v=$a\n    EOT;` prints `v=X` under `php -n` 8.5.6. This is the
/// reduced form of the fourteen refused vendor files.
#[test]
fn parse_fragment_accepts_a_flexible_heredoc_with_a_variable() {
    let program =
        parse_fragment(b"$s = <<<EOT\n    v=$a\n    EOT;\n").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::StoreVar {
            name: "s".to_string(),
            value: EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::Const(EvalConst::String("v=".to_string()))),
                right: Box::new(EvalExpr::LoadVar("a".to_string())),
            },
        }]
    );
}

/// Verifies a heredoc `{$a['k']}` lowers to the same array read a `"…"` literal produces.
///
/// With `$a = ['k' => 'KV']`, `echo <<<EOT\n    key={$a['k']}\n    EOT;` prints `key=KV`.
#[test]
fn parse_fragment_accepts_a_heredoc_with_a_braced_array_read() {
    let program =
        parse_fragment(b"echo <<<EOT\nkey={$a['k']}\nEOT;\n").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Const(EvalConst::String("key=".to_string()))),
            right: Box::new(EvalExpr::ArrayGet {
                array: Box::new(EvalExpr::LoadVar("a".to_string())),
                index: Box::new(EvalExpr::Const(EvalConst::String("k".to_string()))),
            }),
        })]
    );
}

/// Verifies a heredoc `{$o->p}` lowers to a property read.
///
/// PHP prints the property's value for the body `p={$o->p}`.
#[test]
fn parse_fragment_accepts_a_heredoc_with_a_braced_property_read() {
    let program =
        parse_fragment(b"echo <<<EOT\np={$o->p}\nEOT;\n").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Const(EvalConst::String("p=".to_string()))),
            right: Box::new(EvalExpr::PropertyGet {
                object: Box::new(EvalExpr::LoadVar("o".to_string())),
                property: "p".to_string(),
            }),
        })]
    );
}

/// Verifies a nowdoc body stays one literal even though it holds a `$` and a `{$…}`.
///
/// `php -n` 8.5.6 prints `raw $a {$b}` for this body: a nowdoc is a single-quoted body and
/// must not follow the heredoc arm into interpolation.
#[test]
fn parse_fragment_keeps_a_nowdoc_body_literal() {
    let program =
        parse_fragment(b"echo <<<'EOT'\n    raw $a {$b}\n    EOT;\n").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
            "raw $a {$b}".to_string()
        )))]
    );
}

/// Verifies a heredoc body expands escapes but keeps `\"`, as PHP's own scanner does.
///
/// For the body `tab:\t q:\" bs:\\ d:\$v`, PHP prints a real tab, then `q:\"`, then a single
/// backslash, then `$v`. Only the quote diverges from a `"…"` literal, which has a quote to
/// escape and so unescapes it.
#[test]
fn parse_fragment_expands_heredoc_escapes_but_keeps_the_quote() {
    let program = parse_fragment(b"echo <<<EOT\ntab:\\t q:\\\" bs:\\\\ d:\\$v\nEOT;\n")
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
            "tab:\t q:\\\" bs:\\ d:$v".to_string()
        )))]
    );
}

/// Verifies a heredoc closing marker may be followed by an argument separator.
///
/// `echo f(<<<EOT\n    a\n    EOT, 1);` is accepted by PHP: the marker ends the body and the
/// rest of the line keeps parsing as the call it belongs to.
#[test]
fn parse_fragment_accepts_a_heredoc_closed_inside_an_argument_list() {
    let program =
        parse_fragment(b"echo f(<<<EOT\n    a\n    EOT, 1);\n").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::Call {
            name: "f".to_string(),
            args: vec![
                EvalCallArg::positional(EvalExpr::Const(EvalConst::String("a".to_string()))),
                EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(1))),
            ],
        })]
    );
}

/// Verifies an identifier written in UTF-8 above the ASCII range parses as one name.
///
/// PHP's identifier rule is a byte rule: every byte from `0x80` up is a name character, so
/// `class Café { public const V = 5; }` is a legal declaration and `Café::V` fetches it.
#[test]
fn parse_fragment_accepts_a_non_ascii_class_name() {
    let program = parse_fragment("echo Café::V;".as_bytes()).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::ClassConstantFetch {
            class_name: "Café".to_string(),
            constant: "V".to_string(),
        })]
    );
}

/// Verifies a raw non-UTF-8 byte in identifier position parses instead of failing.
///
/// This is the reduced form of the refused `symfony/cache` file. `php -n` 8.5.6 runs
/// `class \xA9 { public const V = 5; } echo \xA9::V;` and prints `5`. The fragment used to
/// fail as invalid UTF-8 before a high byte outside a literal was allowed to become a
/// private-use marker and before the marker counted as an identifier character.
///
/// The class name carries the marker rather than the raw byte, which is what makes the
/// declaration and the fetch below agree on one name.
#[test]
fn parse_fragment_accepts_a_high_byte_class_name() {
    let program = parse_fragment(b"echo \xA9::V;").expect("fragment should parse");
    let marker = char::from_u32(0xF_0000 + 0xA9).expect("marker is a valid code point");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::ClassConstantFetch {
            class_name: marker.to_string(),
            constant: "V".to_string(),
        })]
    );
}

/// Verifies a raw non-UTF-8 byte still reaches a string literal as that exact byte.
///
/// `bin2hex("\xA9")` written as a raw source byte is `a9` under `php -n` 8.5.6. Marking high
/// bytes everywhere rather than only inside quotes must not change what a literal holds.
#[test]
fn parse_fragment_keeps_a_high_byte_literal_as_bytes() {
    let program = parse_fragment(b"echo \"\xA9\";").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Echo(EvalExpr::Const(EvalConst::Bytes(vec![0xA9])))]
    );
}

/// Verifies a `\xFF` escape reaches the interpreter as one byte, not as four characters.
///
/// `bin2hex("\xFF")` is `ff`: the literal is a single byte that is not valid UTF-8, so it
/// can only travel as bytes. `"\377"` names the same byte through the octal form.
#[test]
fn parse_fragment_lowers_a_high_byte_escape_to_bytes() {
    for fragment in [b"echo \"\\xFF\";".as_slice(), b"echo \"\\377\";".as_slice()] {
        let program = parse_fragment(fragment).expect("fragment should parse");
        assert_eq!(
            program.statements(),
            &[EvalStmt::Echo(EvalExpr::Const(EvalConst::Bytes(vec![
                0xFF
            ])))],
            "{}",
            String::from_utf8_lossy(fragment)
        );
    }
}

/// Verifies a malformed code-point escape is a parse error, as it is for PHP.
///
/// `php -n` 8.5.6 reports `Parse error: Invalid UTF-8 codepoint escape sequence` for
/// `"\u{}"` and adds `: Codepoint too large` for `"\u{110000}"`. The lexer reports it as
/// `EvalParseError::InvalidUtf8`, the kind naming that error; what matters most is that the
/// fragment is refused rather than run with invented text.
#[test]
fn parse_fragment_refuses_a_malformed_code_point_escape() {
    for fragment in [
        b"echo \"\\u{}\";".as_slice(),
        b"echo \"\\u{110000}\";".as_slice(),
    ] {
        assert_eq!(
            parse_fragment_error(fragment),
            Err(EvalParseError::InvalidUtf8),
            "{}",
            String::from_utf8_lossy(fragment)
        );
    }
}
