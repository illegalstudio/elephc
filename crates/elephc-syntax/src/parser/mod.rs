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
mod declare_context;
pub mod declaration_groups;
/// Resolves relative `namespace\name` references against the namespace being parsed.
mod relative_names;
mod tokenizer_feedback;
pub mod stmt;

pub use attributes::{consume_attribute_lists, parse_attribute_lists};
pub use relative_names::{
    at_top_level_statement, current_namespace_parts, enter_namespace, enter_statement,
    relative_name_starts_at, restore_namespace,
};

/// Re-exports the root AST node for a parsed PHP file, containing all top-level statements.
pub use ast::Program;

use std::cell::{Cell, RefCell};

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::Stmt;
use crate::span::Span;

/// Shares class-like name token grammar with source attribution tools.
///
/// The compiler binary uses this library API, not its parallel private parser module.
#[allow(dead_code)]
pub fn name_part_from_token(token: &Token, metadata: &crate::lexer::TokenMetadata) -> Option<String> {
    stmt::name_part_from_token(token, metadata)
}

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
    /// While an included file parses: a stable key for that file and the anonymous classes it has
    /// declared so far. Set by `with_anonymous_class_scope`.
    static ANONYMOUS_CLASS_SCOPE: RefCell<Option<(u64, usize)>> = const { RefCell::new(None) };
    /// Whether the current parse only validates syntax and may discard statement ASTs eagerly.
    static VALIDATION_ONLY: Cell<bool> = const { Cell::new(false) };
}

/// Scoped parser mode that drops each top-level statement after it validates.
///
/// TOKEN_PARSE only needs parser success and the recorded identifier feedback. Keeping the
/// whole AST alive for a 500k-token source costs hundreds of megabytes, so validation parses
/// release each declaration before reading the next one.
struct ValidationOnlyGuard {
    previous: bool,
}

impl ValidationOnlyGuard {
    /// Enables validation-only parsing and remembers the previous thread-local state.
    fn start() -> Self {
        let previous = VALIDATION_ONLY.with(|flag| flag.replace(true));
        Self { previous }
    }
}

impl Drop for ValidationOnlyGuard {
    /// Restores the parser mode of the enclosing parse.
    fn drop(&mut self) {
        VALIDATION_ONLY.with(|flag| flag.set(self.previous));
    }
}

/// Returns whether the current parse may release statements after syntax validation.
fn validation_only() -> bool {
    VALIDATION_ONLY.with(Cell::get)
}

/// Returns a fresh synthetic class name for an anonymous class. The `@`/`#` characters cannot
/// appear in a PHP identifier, so the name never collides with a user class, and `mangle_fqn`
/// hex-encodes them when generating assembly symbols.
///
/// Inside `with_anonymous_class_scope` the name is derived from the file and the class's position
/// in it, so every parse of one included file names its anonymous classes alike: the resolver
/// parses an included file once to discover its declarations and again to inline its body, and
/// the two must agree. Elsewhere a process-wide counter keeps the names unique.
pub fn next_anonymous_class_name() -> String {
    let scoped = ANONYMOUS_CLASS_SCOPE.with(|scope| {
        scope.borrow_mut().as_mut().map(|(file, position)| {
            let name = format!("class@anonymous#{:016x}x{}", file, position);
            *position += 1;
            name
        })
    });
    if let Some(name) = scoped {
        return name;
    }
    let id = ANONYMOUS_CLASS_COUNTER.with(|counter| {
        let id = counter.get();
        counter.set(id + 1);
        id
    });
    format!("class@anonymous#{}", id)
}

/// Returns the next anonymous-class number, for [`restore_anonymous_class_counter`].
pub fn anonymous_class_counter() -> usize {
    ANONYMOUS_CLASS_COUNTER.with(|counter| counter.get())
}

/// Rewinds the anonymous-class numbering after a parse whose result is discarded (the autoload
/// prelude probe), so the real parse names its anonymous classes exactly as without it.
pub fn restore_anonymous_class_counter(next: usize) {
    ANONYMOUS_CLASS_COUNTER.with(|counter| counter.set(next));
}

/// Returns whether `name` is a synthetic anonymous-class name from `next_anonymous_class_name`.
pub fn is_anonymous_class_name(name: &str) -> bool {
    name.trim_start_matches('\\').starts_with("class@anonymous#")
}

/// Runs `parse` with anonymous classes named after `file` and their position in it (see
/// `next_anonymous_class_name`), restoring the previous naming afterwards, so a nested include
/// keeps its own count.
pub fn with_anonymous_class_scope<T>(file: &str, parse: impl FnOnce() -> T) -> T {
    // FNV-1a over the file path: stable for a path within and across runs.
    let key = file.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    let previous = ANONYMOUS_CLASS_SCOPE.with(|scope| scope.replace(Some((key, 0))));
    let result = parse();
    ANONYMOUS_CLASS_SCOPE.with(|scope| scope.replace(previous));
    result
}

/// Records a hoisted anonymous-class `ClassDecl` so the current parse appends it to the program.
pub fn register_anonymous_class(decl: Stmt) {
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

/// Parses PHP source for TOKEN_PARSE and returns the keyword tokens used as identifiers.
///
/// The returned spans point at tokens Zend must rewrite from their keyword token ID to
/// T_STRING. A parse error is returned exactly as the ordinary parser reports it.
#[allow(dead_code)]
pub fn parse_with_tokenizer_feedback(
    tokens: &[SpannedToken],
) -> (Result<Program, CompileError>, Vec<(Span, String)>) {
    let feedback = tokenizer_feedback::FeedbackGuard::start();
    let validation = ValidationOnlyGuard::start();
    let result = parse_with_recovery_in_mode(tokens, crate::source::SourceMode::Php)
        .map_err(CompileError::from_many);
    drop(validation);
    let names = feedback.names();
    (result, names)
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
    relative_names::with_global_namespace_scope(|| {
        crate::compiler_stack::with_compiler_stack(|| parse_checked_tokens(tokens))
    })
}

/// Parses tokens whose nesting depth is already known to be within the compiler's limit.
fn parse_checked_tokens(tokens: &[SpannedToken]) -> Result<Program, Vec<CompileError>> {
    let validate_only = validation_only();
    let mut pos = 0;
    let mut stmts = Vec::new();
    let mut errors = Vec::new();
    let mut leading_declares = true;

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
        let allows_strict_types = leading_declares && tokens[pos].0 == Token::Declare;
        leading_declares = allows_strict_types;
        match stmt::parse_file_or_namespace_scope_item(
            tokens,
            &mut pos,
            &mut stmts,
            allows_strict_types,
        ) {
            Ok(()) => {
                if validate_only {
                    stmts.clear();
                }
            }
            Err(error) => {
                errors.extend(error.flatten());
                stmt::recover_to_statement_boundary(tokens, &mut pos);
            }
        }
    }

    if validate_only {
        let _ = take_anonymous_classes();
        return if errors.is_empty() { Ok(Vec::new()) } else { Err(errors) };
    }

    // Append anonymous-class declarations hoisted out of expression position. Their position in
    // the program does not matter: declaration discovery scans all declarations before use.
    stmts.append(&mut take_anonymous_classes());

    if errors.is_empty() {
        if crate::source::current_strict_types() {
            declare_context::mark_main_strict_types(&mut stmts);
        }
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
