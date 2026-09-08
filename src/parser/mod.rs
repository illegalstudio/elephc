//! Purpose:
//! Provides the public parser entry points from spanned tokens to an AST program.
//! Coordinates statement parsing and optional recovery for collecting multiple syntax errors.
//!
//! Called from:
//! - `crate::pipeline::compile()` and `crate::resolver::files::parse_file()`.
//!
//! Key details:
//! - Parser output preserves spans and PHP syntax shape for later passes to rewrite safely.

/// PHP alternative control-structure syntax (`:` … `endif;`) body parsing helpers.
mod alt_syntax;
/// Defines AST node types representing the PHP syntax tree produced by the parser.
pub mod ast;
mod attributes;
/// Control flow statements: `if`, `while`, `for`, `foreach`, `switch`, `try`, `goto`, and `label` parsing.
mod control;
pub mod expr;
/// Maps tokens that may legally appear as bareword names (identifiers and semi-reserved keywords).
mod keyword_name;
mod stmt;

pub(crate) use attributes::{consume_attribute_lists, parse_attribute_lists};

/// Re-exports the root AST node for a parsed PHP file, containing all top-level statements.
pub use ast::Program;

use std::cell::{Cell, RefCell};

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::Stmt;
use crate::span::Span;

/// Caps syntactic delimiter nesting before recursive expression parsing can
/// consume the compiler process stack.
const MAX_COMPILER_NESTING: usize = 1024;

thread_local! {
    /// Anonymous-class declarations (`new class {}`) hoisted out of expression position during
    /// the current parse. Drained into the program by `parse_with_recovery`.
    static ANONYMOUS_CLASSES: RefCell<Vec<Stmt>> = const { RefCell::new(Vec::new()) };
    /// Monotonic counter producing unique synthetic class names. Never reset within a process so
    /// that anonymous classes from different files (e.g. includes) cannot collide once merged.
    static ANONYMOUS_CLASS_COUNTER: Cell<usize> = const { Cell::new(0) };
}

/// Returns a fresh, globally-unique synthetic class name for an anonymous class. The `@`/`#`
/// characters cannot appear in a PHP identifier, so the name never collides with a user class,
/// and `mangle_fqn` hex-encodes them when generating assembly symbols.
pub(crate) fn next_anonymous_class_name() -> String {
    let id = ANONYMOUS_CLASS_COUNTER.with(|counter| {
        let id = counter.get();
        counter.set(id + 1);
        id
    });
    format!("class@anonymous#{}", id)
}

/// Records a hoisted anonymous-class `ClassDecl` so the current parse appends it to the program.
pub(crate) fn register_anonymous_class(decl: Stmt) {
    ANONYMOUS_CLASSES.with(|classes| classes.borrow_mut().push(decl));
}

/// Removes and returns every anonymous-class declaration collected so far in this thread.
fn take_anonymous_classes() -> Vec<Stmt> {
    ANONYMOUS_CLASSES.with(|classes| std::mem::take(&mut *classes.borrow_mut()))
}

/// Parses tokens into an AST program, returning the first error if any.
#[allow(dead_code)]
pub fn parse(tokens: &[SpannedToken]) -> Result<Program, CompileError> {
    parse_with_mode(tokens, crate::source::SourceMode::Php)
}

/// Parses compiler-generated tagged source without exposing it to user strict-PHP rules.
///
/// No compilation path calls this any more: the synthetic preludes it existed for are BUILT
/// rather than parsed, so the only callers left are the oracles that check those builders
/// against the PHP they replaced. The lib keeps it as public API; the bin compiles the same
/// sources through its own module tree, where nothing reaches it — hence the allow, exactly as
/// `parse` above carries it.
#[allow(dead_code)]
pub fn parse_internal(tokens: &[SpannedToken]) -> Result<Program, CompileError> {
    parse_with_mode(tokens, crate::source::SourceMode::Internal)
}

/// Parses tokens under an explicit physical-file source mode.
///
/// Parser-created statements retain the mode so later builtin resolution can
/// distinguish strict PHP from LFC code after include/autoload AST merging.
pub fn parse_with_mode(
    tokens: &[SpannedToken],
    mode: crate::source::SourceMode,
) -> Result<Program, CompileError> {
    match parse_with_recovery_in_mode(tokens, mode) {
        Ok(program) => Ok(program),
        Err(errors) => Err(CompileError::from_many(errors)),
    }
}

/// Parses tokens with recovery, collecting all syntax errors encountered.
#[allow(dead_code)]
pub fn parse_with_recovery(tokens: &[SpannedToken]) -> Result<Program, Vec<CompileError>> {
    parse_with_recovery_in_mode(tokens, crate::source::SourceMode::Php)
}

/// Parses tokens with recovery while tagging parser-created statements with `mode`.
pub fn parse_with_recovery_in_mode(
    tokens: &[SpannedToken],
    mode: crate::source::SourceMode,
) -> Result<Program, Vec<CompileError>> {
    crate::source::with_parse_mode(crate::source::SourceProfile::new(mode), || {
        parse_with_recovery_inner(tokens)
    })
}

/// Implements recovery parsing after the source-mode scope has been installed.
fn parse_with_recovery_inner(tokens: &[SpannedToken]) -> Result<Program, Vec<CompileError>> {
    reject_excessive_nesting(tokens)?;
    let mut pos = 0;
    let mut stmts = Vec::new();
    let mut errors = Vec::new();

    // Discard any anonymous classes left over from a previous parse that errored before draining.
    let _ = take_anonymous_classes();

    // Skip OpenTag
    if pos < tokens.len() && tokens[pos].0 == Token::OpenTag {
        pos += 1;
    } else {
        let span = if pos < tokens.len() {
            tokens[pos].1.span
        } else {
            Span::dummy()
        };
        return Err(vec![CompileError::new(span, "Expected '<?php' open tag")]);
    }

    while pos < tokens.len() {
        if tokens[pos].0 == Token::Eof {
            break;
        }
        if tokens[pos].0 == Token::Semicolon {
            pos += 1;
            continue;
        }
        // PHP permits standalone compound statements. Blocks do not introduce
        // a lexical scope, so their statements can be flattened into the
        // surrounding statement list.
        if tokens[pos].0 == Token::LBrace {
            match stmt::parse_block(tokens, &mut pos) {
                Ok(mut block_stmts) => stmts.append(&mut block_stmts),
                Err(error) => errors.extend(error.flatten()),
            }
        // Extern blocks can produce multiple stmts. Attributes on declarations
        // flow through parse_stmt below — extern is an elephc-specific block
        // that does not interact with PHP attributes.
        } else if tokens[pos].0 == Token::Extern {
            match stmt::parse_extern_stmts(tokens, &mut pos) {
                Ok(mut extern_stmts) => stmts.append(&mut extern_stmts),
                Err(error) => {
                    errors.extend(error.flatten());
                    stmt::recover_to_statement_boundary(tokens, &mut pos);
                }
            }
        } else {
            match stmt::parse_stmt(tokens, &mut pos) {
                Ok(stmt) => stmts.push(stmt),
                Err(error) => {
                    errors.extend(error.flatten());
                    stmt::recover_to_statement_boundary(tokens, &mut pos);
                }
            }
        }
    }

    // Append anonymous-class declarations hoisted out of expression position. Their position in
    // the program does not matter: declaration discovery scans all declarations before use.
    stmts.append(&mut take_anonymous_classes());

    if errors.is_empty() {
        Ok(stmts)
    } else {
        Err(errors)
    }
}

/// Rejects syntactically nested delimiters before recursive parser routines
/// can overflow the compiler stack on hostile source input.
fn reject_excessive_nesting(tokens: &[SpannedToken]) -> Result<(), Vec<CompileError>> {
    let mut depth = 0usize;
    for (token, location) in tokens {
        match token {
            Token::LParen | Token::LBrace | Token::LBracket => {
                depth += 1;
                if depth > MAX_COMPILER_NESTING {
                    return Err(vec![CompileError::new(
                        location.span,
                        "maximum compiler nesting depth exceeded",
                    )]);
                }
            }
            Token::RParen | Token::RBrace | Token::RBracket => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    Ok(())
}
