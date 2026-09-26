//! Purpose:
//! Expands PHP relative names (`namespace\foo`) into fully qualified names before parsing.
//!
//! Called from:
//! - `crate::parser::parse_with_recovery_inner()`.
//!
//! Key details:
//! - `namespace\foo` means "foo in the current namespace", so it is exactly `\Current\Ns\foo`,
//!   and plain `\foo` in the global namespace. Rewriting the tokens keeps every name consumer
//!   (calls, `new`, constants, `instanceof`, type hints) on the fully qualified path it has.
//! - The current namespace is the one the most recent `namespace X;` / `namespace X {` /
//!   `namespace {` declaration opened. Each braced block starts with its own declaration, and
//!   code outside the blocks is not allowed in that form, so the latest declaration is always
//!   the enclosing one.

use crate::lexer::{SpannedToken, Token, TokenMetadata};

use super::stmt::name_part_from_token;

/// Returns the token stream with every `namespace\` relative prefix replaced by the current
/// namespace's fully qualified prefix, or `None` when the stream has no relative name.
pub(super) fn expand_relative_names(tokens: &[SpannedToken]) -> Option<Vec<SpannedToken>> {
    if !(0..tokens.len()).any(|index| is_relative_prefix(tokens, index)) {
        return None;
    }
    let mut out = Vec::with_capacity(tokens.len() + 8);
    let mut namespace: Vec<String> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if is_relative_prefix(tokens, index) {
            // `namespace` `\` becomes `\` `Part` `\` ... `Part` `\`; the original backslash
            // stays and joins the last namespace part to the rest of the name.
            let metadata = &tokens[index].1;
            out.push((Token::Backslash, TokenMetadata::new(metadata.span)));
            for part in &namespace {
                out.push((Token::Identifier(part.clone()), TokenMetadata::new(metadata.span)));
                out.push((Token::Backslash, TokenMetadata::new(metadata.span)));
            }
            index += 2;
            continue;
        }
        if let Some(declared) = namespace_declaration(tokens, index) {
            namespace = declared;
        }
        out.push(tokens[index].clone());
        index += 1;
    }
    Some(out)
}

/// Returns true when `namespace` at `index` is the relative-name prefix `namespace\`.
fn is_relative_prefix(tokens: &[SpannedToken], index: usize) -> bool {
    matches!(tokens.get(index), Some((Token::Namespace, _)))
        && matches!(tokens.get(index + 1), Some((Token::Backslash, _)))
        && !follows_member_access(tokens, index)
}

/// Returns the namespace parts a declaration at `index` opens (empty for the global
/// `namespace {`), or `None` when the token there is not a namespace declaration.
fn namespace_declaration(tokens: &[SpannedToken], index: usize) -> Option<Vec<String>> {
    if !matches!(tokens.get(index), Some((Token::Namespace, _))) || follows_member_access(tokens, index)
    {
        return None;
    }
    let mut parts = Vec::new();
    let mut cursor = index + 1;
    loop {
        let (token, metadata) = tokens.get(cursor)?;
        match token {
            Token::Semicolon | Token::LBrace => return Some(parts),
            Token::Backslash if !parts.is_empty() => cursor += 1,
            _ => {
                parts.push(name_part_from_token(token, metadata)?);
                cursor += 1;
            }
        }
    }
}

/// Returns true when the token before `index` makes it a member or method name
/// (`$o->namespace`, `C::namespace`, `function namespace`), not the keyword.
fn follows_member_access(tokens: &[SpannedToken], index: usize) -> bool {
    index > 0
        && matches!(
            tokens[index - 1].0,
            Token::Arrow | Token::QuestionArrow | Token::DoubleColon | Token::Function
        )
}
