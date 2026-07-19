//! Purpose:
//! Provides the public parser entry points from spanned tokens to an AST program.
//! Coordinates statement parsing and optional recovery for collecting multiple syntax errors.
//!
//! Called from:
//! - `crate::pipeline::compile()` and `crate::resolver::files::parse_file()`.
//!
//! Key details:
//! - Parser output preserves spans and PHP syntax shape for later passes to rewrite safely.

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
pub fn parse(tokens: &[SpannedToken]) -> Result<Program, CompileError> {
    match parse_with_recovery(tokens) {
        Ok(program) => Ok(program),
        Err(errors) => Err(CompileError::from_many(errors)),
    }
}

/// Parses tokens with recovery, collecting all syntax errors encountered.
pub fn parse_with_recovery(tokens: &[SpannedToken]) -> Result<Program, Vec<CompileError>> {
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
        // Extern blocks can produce multiple stmts. Attributes on declarations
        // flow through parse_stmt below — extern is an elephc-specific block
        // that does not interact with PHP attributes.
        if tokens[pos].0 == Token::Extern {
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
