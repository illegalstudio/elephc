//! Purpose:
//! Parser tests for `goto` statements, jump labels, and the jump validation PHP performs when it
//! compiles a fragment.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - An invalid jump refuses the whole fragment as `EvalParseError::InvalidJump`, before any of
//!   it runs, matching the order of PHP's compile-time fatal errors.
//! - Each refused fragment below is a compile-time fatal error under PHP 8.5.12 as well.

use super::support::*;

/// Verifies `goto name;` and `name:` parse into their own statements in source order.
#[test]
fn parse_fragment_accepts_goto_and_label_source() {
    let program = parse_fragment(br#"goto done; echo 1; done: echo 2;"#).expect("fragment should parse");
    let statements = program.statements();
    assert_eq!(statements.len(), 4);
    assert_eq!(statements[0], EvalStmt::Goto("done".to_string()));
    assert_eq!(statements[2], EvalStmt::Label("done".to_string()));
}

/// Verifies the keyword is case-insensitive while a label keeps its spelling.
#[test]
fn parse_fragment_goto_keyword_is_case_insensitive() {
    let program = parse_fragment(br#"GOTO Done; Done:"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Goto("Done".to_string()), EvalStmt::Label("Done".to_string())]
    );
    assert_eq!(
        parse_fragment(br#"goto Done; done: echo 1;"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies the other `name` `:` positions are not read as labels.
#[test]
fn parse_fragment_does_not_confuse_labels_with_other_colons() {
    for source in [
        &br#"$a = $flag ? A : B;"#[..],
        br#"switch ($x) { case A: echo 1; break; default: echo 2; }"#,
        br#"if ($flag): echo 1; else: echo 2; endif;"#,
        br#"echo Foo::BAR;"#,
    ] {
        let program = parse_fragment(source).expect("fragment should parse");
        assert!(
            !format!("{:?}", program.statements()).contains("Label("),
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

/// Verifies `goto` takes a plain label name and a semicolon, never an expression.
#[test]
fn parse_fragment_rejects_malformed_goto_source() {
    assert!(parse_fragment(br#"goto;"#).is_err());
    assert!(parse_fragment(br#"goto $label;"#).is_err());
    assert!(parse_fragment(br#"goto 1;"#).is_err());
    assert!(parse_fragment(br#"goto done"#).is_err());
}

/// Verifies a jump to a label that does not exist refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_undefined_label() {
    assert_eq!(
        parse_fragment(br#"goto nope;"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies a jump out of a function body to a label of the fragment refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_label_in_another_function() {
    assert_eq!(
        parse_fragment(br#"function f() { goto outside; } outside: echo 1;"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies a jump out of a closure body refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_label_outside_closure() {
    assert_eq!(
        parse_fragment(br#"$f = function () { goto outside; }; outside: echo 1;"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies a label declared twice in one scope refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_duplicate_label() {
    assert_eq!(
        parse_fragment(br#"a: echo 1; a: goto a;"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies a jump into a while loop refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_into_while() {
    assert_eq!(
        parse_fragment(br#"goto in; while (true) { in: break; }"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies a jump into a foreach loop refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_into_foreach() {
    assert_eq!(
        parse_fragment(br#"goto in; foreach ([1] as $v) { in: echo $v; }"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies a jump into a switch from outside it refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_into_switch() {
    assert_eq!(
        parse_fragment(br#"goto in; switch (1) { case 1: in: echo 1; }"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies a jump into a finally block refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_into_finally() {
    assert_eq!(
        parse_fragment(br#"goto f; try { echo 1; } finally { f: echo 2; }"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies a jump out of a finally block refuses the fragment.
#[test]
fn parse_fragment_rejects_goto_out_of_finally() {
    assert_eq!(
        parse_fragment(br#"try { echo 1; } finally { goto o; } o: echo 3;"#),
        Err(EvalParseError::InvalidJump)
    );
}

/// Verifies jumps PHP accepts stay accepted: out of loops and `try`, into an `if` branch, into a
/// `try` body, and within one `finally` block.
#[test]
fn parse_fragment_accepts_valid_jump_shapes() {
    for source in [
        &br#"while (true) { goto out; } out: echo 1;"#[..],
        br#"try { goto out; } finally { echo 1; } out: echo 2;"#,
        br#"goto in; if ($flag) { in: echo 1; }"#,
        br#"goto in; try { in: echo 1; } catch (Exception $e) { echo 2; }"#,
        br#"try { echo 1; } finally { again: $i = $i + 1; if ($i < 3) { goto again; } }"#,
        br#"switch ($x) { case 1: again: $i = $i + 1; if ($i < 3) { goto again; } }"#,
    ] {
        assert!(parse_fragment(source).is_ok(), "{}", String::from_utf8_lossy(source));
    }
}
