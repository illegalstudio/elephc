//! Purpose:
//! Holds source and caller-scope metadata for runtime PHP fragments.
//!
//! Called from:
//! - `crate::parser::parse_fragment()`.
//!
//! Key details:
//! - Generic specialization carries the declaration's imports and strict mode.
//! - Tokens retain CLI superglobal usage and the caller's variable access mask.

use super::names::split_first_name_segment;
use crate::eval_ir::{
    eval_scope_name_mask, EvalProgram, EVAL_CLI_POPULATED_SUPERGLOBALS,
    EVAL_SCOPE_ACCESS_ALL,
};
use crate::lexer::{Token, TokenKind};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

static ANONYMOUS_CLASS_COUNTER: AtomicUsize = AtomicUsize::new(0);
static CLOSURE_FUNCTION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Retains source tokens and lexical scope for Chopper and EvalIR lowering.
pub(super) struct FragmentInput {
    /// `declare(strict_types=...)` active for this eval fragment.
    pub(super) strict_types: bool,
    pub(super) tokens: Vec<TokenKind>,
    pub(super) token_lines: Vec<i64>,
    pub(super) token_phpdoc: Vec<bool>,
    pub(super) source_len: usize,
    pub(super) namespace: String,
    pub(super) imports: NamespaceImports,
}

/// Import alias tables active for the current namespace declaration region.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq)]
pub(crate) struct NamespaceImports {
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

/// Returns a parser-global synthetic class name for one eval anonymous class expression.
pub(crate) fn next_anonymous_class_name() -> String {
    let id = ANONYMOUS_CLASS_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("class@anonymous#eval{id}")
}

/// Returns a parser-global synthetic function name for one eval closure expression.
pub(crate) fn next_closure_function_name() -> String {
    let id = CLOSURE_FUNCTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{{closure:eval:function:{id}}}")
}

impl NamespaceImports {
    /// Stores one class import under PHP's case-insensitive class alias key.
    pub(crate) fn insert_class(&mut self, alias: String, name: String) {
        self.classes.insert(alias.to_ascii_lowercase(), name);
    }

    /// Stores one function import under PHP's case-insensitive function alias key.
    pub(crate) fn insert_function(&mut self, alias: String, name: String) {
        self.functions.insert(alias.to_ascii_lowercase(), name);
    }

    /// Stores one constant import under PHP's case-sensitive constant alias key.
    pub(crate) fn insert_constant(&mut self, alias: String, name: String) {
        self.constants.insert(alias, name);
    }

    /// Resolves a class import, including aliases used as the first segment of a class name.
    pub(crate) fn resolve_class(&self, name: &str) -> Option<String> {
        let (first, tail) = split_first_name_segment(name);
        let imported = self.classes.get(&first.to_ascii_lowercase())?;
        Some(match tail {
            Some(tail) => format!("{imported}\\{tail}"),
            None => imported.clone(),
        })
    }

    /// Resolves an unqualified function alias.
    pub(crate) fn resolve_function(&self, name: &str) -> Option<&str> {
        self.functions
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Resolves a case-sensitive unqualified constant alias.
    pub(crate) fn resolve_constant(&self, name: &str) -> Option<&str> {
        self.constants.get(name).map(String::as_str)
    }
}

impl FragmentInput {
    /// Creates a parser over tokens produced from a source fragment.
    pub(super) fn new(tokens: Vec<Token>, source_len: usize) -> Self {
        let tokens = super::docblocks::apply(tokens);
        let token_phpdoc = tokens.iter().map(Token::phpdoc).collect();
        let token_lines = tokens.iter().map(Token::line).collect();
        let tokens = tokens.into_iter().map(Token::into_kind).collect();
        Self {
            strict_types: false,
            tokens,
            token_lines,
            token_phpdoc,
            source_len,
            namespace: String::new(),
            imports: NamespaceImports::default(),
        }
    }

    /// Preserves token-based metadata for EvalIR produced by Chopper.
    pub(super) fn finish_program(&self, statements: Vec<crate::eval_ir::EvalStmt>) -> EvalProgram {
        let program = EvalProgram::new(self.source_len, statements);
        program.with_cli_superglobals(self.named_cli_superglobals())
            .with_scope_access_mask(self.scope_access_mask())
            .with_strict_types(self.strict_types)
    }

    /// Builds a conservative mask of caller-scope variable names named by this fragment.
    fn scope_access_mask(&self) -> u64 {
        if !self.has_direct_scalar_scope_access_only() {
            return EVAL_SCOPE_ACCESS_ALL;
        }
        let mut mask = 0_u64;
        for token in &self.tokens {
            match token {
                TokenKind::DollarIdent(name) if name == "GLOBALS" => {
                    return EVAL_SCOPE_ACCESS_ALL;
                }
                TokenKind::DollarIdent(name) => mask |= eval_scope_name_mask(name),
                _ => {}
            }
        }
        mask
    }

    /// Accepts only direct numeric expressions that cannot retain indirect scope access.
    fn has_direct_scalar_scope_access_only(&self) -> bool {
        for (index, token) in self.tokens.iter().enumerate() {
            if !matches!(
                token,
                TokenKind::DollarIdent(_)
                    | TokenKind::Int(_)
                    | TokenKind::Float(_)
                    | TokenKind::Plus
                    | TokenKind::PlusPlus
                    | TokenKind::PlusEqual
                    | TokenKind::Minus
                    | TokenKind::MinusMinus
                    | TokenKind::MinusEqual
                    | TokenKind::Star
                    | TokenKind::StarStar
                    | TokenKind::StarStarEqual
                    | TokenKind::StarEqual
                    | TokenKind::Slash
                    | TokenKind::SlashEqual
                    | TokenKind::Percent
                    | TokenKind::PercentEqual
                    | TokenKind::Equal
                    | TokenKind::Semicolon
                    | TokenKind::LParen
                    | TokenKind::RParen
                    | TokenKind::Eof
            ) {
                return false;
            }
            if matches!(token, TokenKind::LParen)
                && index > 0
                && matches!(
                    self.tokens[index - 1],
                    TokenKind::DollarIdent(_) | TokenKind::RParen
                )
            {
                return false;
            }
        }
        true
    }

    /// Returns the CLI-populated superglobals any `$name` token of the fragment spells.
    ///
    /// String interpolation is lexed into ordinary variable tokens, so `"$_SERVER[argv]"`
    /// counts too; a dynamic `${'_ENV'}` name does not, since no token spells it.
    fn named_cli_superglobals(&self) -> Vec<&'static str> {
        EVAL_CLI_POPULATED_SUPERGLOBALS
            .into_iter()
            .filter(|superglobal| {
                self.tokens.iter().any(|token| {
                    matches!(token, TokenKind::DollarIdent(name) if name == superglobal)
                })
            })
            .collect()
    }
}
