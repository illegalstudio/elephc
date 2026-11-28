//! Purpose:
//! Parses runtime PHP eval fragments into EvalIR statement form.
//! The module entry point validates fragment boundaries, delegates tokenization,
//! and hands tokens to focused parser state.
//!
//! Called from:
//! - `crate::ffi::execute::__elephc_eval_execute()`
//! - `crate::interpreter` tests and nested eval execution paths.
//!
//! Key details:
//! - PHP eval fragments are statement fragments and must not include opening
//!   `<?` / `<?php` tags.
//! - File and directory metadata are supplied by the eval context at execution time.
//! - Parsing runs on a dedicated temporary stack before any PHP execution can suspend.

pub(crate) mod generics;
mod docblocks;
mod names;
mod default_validation;
pub(crate) mod jumps;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod repl;
pub(crate) mod state;

#[cfg(test)]
mod tests;

use crate::errors::EvalParseError;
use crate::eval_ir::EvalProgram;
use crate::lexer::{tokenize_with_semantics, TokenKind};
use elephc_chopper::syntax::errors::{CompileError, CompileErrorKind};
use state::FragmentInput;

pub(crate) use names::is_reserved_class_like_name as is_reserved_word;
pub(crate) use names::is_reserved_keyword;
pub(crate) use default_validation::method_parameter_default_is_supported;

/// Parser frames must fit independently of the caller's native Fiber or host-thread stack.
const PARSER_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Identifies where the shared frontend refused a runtime fragment.
#[derive(Debug)]
pub(crate) enum SharedParseFailure {
    Tokens,
    Syntax(CompileError),
    Lowering,
    InvalidJump(EvalParseError),
}

impl SharedParseFailure {
    /// Detects incomplete statement terminators without inferring them from message text.
    #[cfg(test)]
    fn missing_final_semicolon(&self, tokens: &[TokenKind]) -> bool {
        matches!(self, Self::Syntax(error) if error.kind == CompileErrorKind::ExpectedSemicolon)
            && self.at_eof(tokens)
    }

    /// Uses the preserved token ordinal to distinguish incomplete input from an earlier error.
    fn at_eof(&self, tokens: &[TokenKind]) -> bool {
        matches!(self, Self::Syntax(error) if usize::try_from(error.span.col).ok() == Some(tokens.len())
            && matches!(tokens.last(), Some(TokenKind::Eof)))
    }

    /// Projects the shared grammar diagnostic to the runtime status contract.
    fn eval_error(&self, tokens: &[TokenKind]) -> EvalParseError {
        match self {
            Self::Syntax(error) => match error.kind {
                CompileErrorKind::ExpectedSemicolon => EvalParseError::ExpectedSemicolon,
                CompileErrorKind::ExpectedVariable => EvalParseError::ExpectedVariable,
                CompileErrorKind::UnsupportedRuntimeConstruct => EvalParseError::UnsupportedConstruct,
                CompileErrorKind::SemanticViolation(elephc_semantics::StrictFeature::Overflow) => {
                    EvalParseError::IntegerLiteralOverflow
                }
                _ if self.at_eof(tokens) => EvalParseError::UnexpectedEof,
                _ => EvalParseError::UnexpectedToken,
            },
            Self::Tokens | Self::Lowering => EvalParseError::UnsupportedConstruct,
            Self::InvalidJump(error) => error.clone(),
        }
    }
}

/// Parses an eval fragment into by-name EvalIR statements.
///
/// A fragment used to be refused outright when it contained `<?` ANYWHERE, a string
/// included: `eval('echo "<?xml";')` failed where reference PHP 8.5.10 prints `<?xml`. The
/// lexer now owns the tags: after a `?>` it reads inline HTML up to the next `<?php`, and a
/// `<?php` met in code lexes as `<` then `?`, which the grammar refuses as PHP does
/// (`eval('<?php echo 1;')` is a parse error there too).
pub fn parse_fragment(code: &[u8]) -> Result<EvalProgram, EvalParseError> {
    parse_fragment_with_semantics(code, elephc_semantics::SemanticProfile::PERMISSIVE)
}

/// Parses runtime source without discarding profile-dependent literal evidence.
pub fn parse_fragment_with_semantics(code: &[u8], profile: elephc_semantics::SemanticProfile) -> Result<EvalProgram, EvalParseError> {
    with_parser_stack(|| {
        let source = std::str::from_utf8(code).map_err(|_| EvalParseError::InvalidUtf8)?;
        let tokens = tokenize_with_semantics(source, profile)?;
        validate_semantic_tokens(&tokens, profile)?;
        parse_runtime_tokens_with_semantics(tokens, code.len(), profile)
    })
}

/// Builds EvalIR and the caller-scope metadata for a fragment parsed by Chopper.
fn lower_shared_program(parser: &FragmentInput) -> Result<EvalProgram, SharedParseFailure> {
    let (statements, strict_types, native_generics) = crate::lower::lower_fragment_in_scope(
        &parser.tokens, &parser.token_lines, &parser.token_phpdoc,
        &parser.namespace, &parser.imports, parser.strict_types,
    )?;
    if parser.tokens.iter().any(|token| {
        matches!(token, TokenKind::Colon)
            || matches!(token, TokenKind::Ident(name) if name.eq_ignore_ascii_case("goto"))
    }) {
        jumps::validate_jump_scope(&statements).map_err(SharedParseFailure::InvalidJump)?;
    }
    Ok(parser.finish_program(statements).with_strict_types(strict_types).with_native_generic_syntax(native_generics))
}

/// Parses a token stream assembled by the caller, ending in an EOF token.
///
/// An included file's stream is built from several blocks and the inline HTML between them
/// (`crate::script_cache::segments::parse_script`), and has to reach the grammar as one
/// program so that a block may span the tags. `source_len` is the whole file's length.
#[cfg(test)]
pub(crate) fn parse_tokens(
    tokens: Vec<crate::lexer::Token>,
    source_len: usize,
) -> Result<EvalProgram, EvalParseError> {
    parse_tokens_with_semantics(tokens, source_len, elephc_semantics::SemanticProfile::PERMISSIVE)
}

/// Parses already-profiled script tokens while retaining their normalization provenance.
pub(crate) fn parse_tokens_with_semantics(
    tokens: Vec<crate::lexer::Token>, source_len: usize, profile: elephc_semantics::SemanticProfile,
) -> Result<EvalProgram, EvalParseError> {
    with_parser_stack(|| {
        validate_semantic_tokens(&tokens, profile)?;
        parse_runtime_tokens_with_semantics(tokens, source_len, profile)
    })
}

/// Rejects forbidden equality syntax before branch pruning or declaration execution.
fn validate_semantic_tokens(tokens: &[crate::lexer::Token], profile: elephc_semantics::SemanticProfile)
    -> Result<(), EvalParseError> {
    use crate::lexer::TokenKind;
    if profile.contains(elephc_semantics::StrictFeature::Comparisons)
        && (tokens.iter().any(|token| matches!(token.kind(), TokenKind::EqualEqual | TokenKind::NotEqual))
            || tokens.windows(2).any(|pair| matches!((pair[0].kind(), pair[1].kind()),
                (TokenKind::Less, TokenKind::Greater))))
    { return Err(EvalParseError::StrictComparisonSyntax); }
    Ok(())
}

/// Lowers shared syntax while stamping the immutable semantic policy on the program.
fn parse_runtime_tokens_with_semantics(
    tokens: Vec<crate::lexer::Token>,
    source_len: usize,
    profile: elephc_semantics::SemanticProfile,
) -> Result<EvalProgram, EvalParseError> {
    let parser = FragmentInput::new(tokens, source_len);
    lower_shared_program(&parser)
        .map(|program| program.with_semantic_profile(profile))
        .map_err(|failure| failure.eval_error(&parser.tokens))
}

/// Uses an explicit stack because native coroutine switches do not update OS-thread stack bounds.
/// Parsing invokes no PHP callbacks; the temporary stack ends before interpreter execution begins.
fn with_parser_stack<R>(parse: impl FnOnce() -> R) -> R {
    stacker::grow(PARSER_STACK_BYTES, parse)
}
