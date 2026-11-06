//! Purpose:
//! Classifies interactive input using the same lexer and grammar as dynamic eval.
//!
//! Called from:
//! - `crate::repl` before submitting a complete fragment to the native eval host.
//!
//! Key details:
//! - Parsing never executes input. Expression wrapping is accepted only as one return.
//! - EOF at the parser's actual failure cursor distinguishes continuation from bad syntax.

use super::{cursor::assignment_op, parse_fragment, state::Parser};
use crate::errors::EvalParseError;
use crate::eval_ir::{EvalProgram, EvalStmt};
use crate::lexer::{tokenize, TokenKind};

/// The terminal action selected for one accumulated input buffer.
#[derive(Debug, PartialEq)]
pub(crate) enum Input {
    Empty,
    Incomplete,
    Ready { source: String, display: bool },
    Invalid { error: EvalParseError, line: i64 },
}

/// Recognizes a complete expression or statement fragment without evaluating either.
pub(crate) fn classify(source: &str) -> Input {
    let tokens = match tokenize(source) {
        Ok(tokens) => tokens,
        Err(EvalParseError::UnterminatedString | EvalParseError::UnterminatedComment) => {
            return Input::Incomplete;
        }
        Err(error) => return Input::Invalid { error, line: source.lines().count().max(1) as i64 },
    };
    if tokens.iter().all(|token| matches!(token.kind(), TokenKind::Eof | TokenKind::Semicolon)) {
        return Input::Empty;
    }
    if tokens.iter().any(|token| matches!(token.kind(), TokenKind::DollarIdent(name) if name == "__elephc_repl_error")) {
        return Input::Invalid { error: EvalParseError::UnsupportedConstruct, line: 1 };
    }
    let simple_assignment = tokens.first().is_some_and(|token| matches!(token.kind(), TokenKind::DollarIdent(_)))
        && tokens.get(1).is_some_and(|token| assignment_op(token.kind()).is_some());
    let mut parser = Parser::new(tokens, source.len());
    match parser.parse_program() {
        Ok(program) => ready(source.to_string(), &program, simple_assignment),
        Err(error) => {
            let at_eof = matches!(parser.current(), TokenKind::Eof);
            // Permit the omitted final semicolon only if it completes the entire grammar.
            // Do not repair syntax before the failure cursor or execute a valid prefix.
            if at_eof && error == EvalParseError::ExpectedSemicolon {
                let terminated = format!("{source}\n;");
                if let Ok(program) = parse_fragment(terminated.as_bytes()) {
                    return ready(terminated, &program, simple_assignment);
                }
            }
            // Anonymous function expressions can resemble incomplete declarations.
            if let Some(expression) = expression_source(source) {
                return Input::Ready { source: expression, display: true };
            }
            if at_eof && error != EvalParseError::UnsupportedConstruct {
                Input::Incomplete
            } else {
                Input::Invalid { error, line: parser.current_line() }
            }
        }
    }
}

/// Displays simple assignment results by reading the assigned variable without repeating its RHS.
fn ready(mut source: String, program: &EvalProgram, simple_assignment: bool) -> Input {
    if matches!(program.statements(), [EvalStmt::Expr(_)]) {
        if let Some(expression) = expression_source(&source) {
            return Input::Ready { source: expression, display: true };
        }
    }
    if simple_assignment {
        if let [EvalStmt::StoreVar { name, .. }] = program.statements() {
            source.push_str(&format!("\nreturn ${name};"));
            return Input::Ready { source, display: true };
        }
    }
    Input::Ready { source, display: matches!(program.statements(), [EvalStmt::Return(Some(_))]) }
}

/// Captures one expression, keeping a trailing line comment clear of the terminator.
fn expression_source(source: &str) -> Option<String> {
    let expression = format!("return {source}\n;");
    let program = parse_fragment(expression.as_bytes()).ok()?;
    matches!(program.statements(), [EvalStmt::Return(Some(_))]).then_some(expression)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises expression capture, comments, declarations, and explicitly returned values.
    #[test]
    fn repl_complete_fragments() {
        for source in ["1 + 2", "$n = 4;", "$n += 2", "[1, 2] // result", "return 3;"] {
            assert!(matches!(classify(source), Input::Ready { display: true, .. }), "{source}");
        }
        for source in ["echo 1", "$n++", "unset($n);", "echo 1; echo 2;", "function f() { return 1; }", "class A {}"] {
            assert!(matches!(classify(source), Input::Ready { display: false, .. }), "{source}");
        }
        assert_eq!(classify(" // comment\n;"), Input::Empty);
    }

    /// Continuation follows the parser through nesting, alternative syntax, and lexical states.
    #[test]
    fn repl_incomplete_fragments() {
        for source in ["function f(", "function f() {", "if (true) { echo 1;", "if (true):", "$a = [1,", "$a =", "1 +", "echo \"open", "/* open"] {
            assert_eq!(classify(source), Input::Incomplete, "{source}");
        }
        assert!(matches!(classify("if (true):\necho 1;\nendif;"), Input::Ready { .. }));
        assert!(matches!(classify("$s = \"line one\nline two\";"), Input::Ready { .. }));
    }

    /// Syntax errors before EOF must never consume later commands as continuation.
    #[test]
    fn repl_invalid_fragments() {
        for source in ["$a = ;", "echo );", "1 2", "function f(]", "<?php echo 1;"] {
            assert!(matches!(classify(source), Input::Invalid { .. }), "{source}: {:?}", classify(source));
        }
    }
}
