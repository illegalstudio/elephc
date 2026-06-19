//! Purpose:
//! Parses tokenized eval fragments into EvalIR.
//! The parser state owns PHP statement and expression grammar for the runtime
//! eval subset after tokenization has completed.
//!
//! Called from:
//! - `crate::parser::parse_fragment()`.
//!
//! Key details:
//! - Namespace imports are tracked as parser state and restored across blocks.
//! - Unsupported PHP constructs fail with explicit parse statuses instead of
//!   partially lowering ambiguous syntax.

use super::cursor::split_first_name_segment;
use crate::errors::EvalParseError;
use crate::eval_ir::EvalProgram;
use crate::lexer::TokenKind;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

static ANONYMOUS_CLASS_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Parses tokenized eval fragments into EvalIR.
pub(super) struct Parser {
    pub(super) tokens: Vec<TokenKind>,
    pub(super) pos: usize,
    pub(super) source_len: usize,
    pub(super) namespace: String,
    pub(super) imports: NamespaceImports,
    pub(super) allow_use_imports: bool,
}

/// A parsed PHP name plus whether it used a leading global namespace separator.
pub(super) struct ParsedQualifiedName {
    pub(super) name: String,
    pub(super) absolute: bool,
}

/// Import alias tables active for the current namespace declaration region.
#[derive(Default)]
pub(super) struct NamespaceImports {
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

/// The `use` declaration namespace being imported.
#[derive(Copy, Clone, Eq, PartialEq)]
pub(super) enum UseImportKind {
    Class,
    Function,
    Const,
}

/// Returns a parser-global synthetic class name for one eval anonymous class expression.
pub(super) fn next_anonymous_class_name() -> String {
    let id = ANONYMOUS_CLASS_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("class@anonymous#eval{id}")
}

impl NamespaceImports {
    /// Stores one class import under PHP's case-insensitive class alias key.
    pub(super) fn insert_class(&mut self, alias: String, name: String) {
        self.classes.insert(alias.to_ascii_lowercase(), name);
    }

    /// Stores one function import under PHP's case-insensitive function alias key.
    pub(super) fn insert_function(&mut self, alias: String, name: String) {
        self.functions.insert(alias.to_ascii_lowercase(), name);
    }

    /// Stores one constant import under PHP's case-sensitive constant alias key.
    pub(super) fn insert_constant(&mut self, alias: String, name: String) {
        self.constants.insert(alias, name);
    }

    /// Resolves a class import, including aliases used as the first segment of a class name.
    pub(super) fn resolve_class(&self, name: &str) -> Option<String> {
        let (first, tail) = split_first_name_segment(name);
        let imported = self.classes.get(&first.to_ascii_lowercase())?;
        Some(match tail {
            Some(tail) => format!("{imported}\\{tail}"),
            None => imported.clone(),
        })
    }

    /// Resolves an unqualified function alias.
    pub(super) fn resolve_function(&self, name: &str) -> Option<&str> {
        self.functions
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Resolves a case-sensitive unqualified constant alias.
    pub(super) fn resolve_constant(&self, name: &str) -> Option<&str> {
        self.constants.get(name).map(String::as_str)
    }
}

impl Parser {
    /// Creates a parser over tokens produced from a source fragment.
    pub(super) fn new(tokens: Vec<TokenKind>, source_len: usize) -> Self {
        Self {
            tokens,
            pos: 0,
            source_len,
            namespace: String::new(),
            imports: NamespaceImports::default(),
            allow_use_imports: true,
        }
    }

    /// Parses a complete eval fragment until EOF.
    pub(super) fn parse_program(mut self) -> Result<EvalProgram, EvalParseError> {
        let mut statements = Vec::new();
        while !matches!(self.current(), TokenKind::Eof) {
            statements.extend(self.parse_stmt()?);
        }
        Ok(EvalProgram::new(self.source_len, statements))
    }
}
