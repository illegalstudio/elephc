//! Purpose:
//! Turns a reserved word that is really a segment of a qualified name (`Default\Palette`,
//! `Vendor\List\Item`) into a plain identifier token, as PHP 8's lexer does.
//!
//! Called from:
//! - `crate::lexer::scan::scan_tokens_in_source()`, after a source (or an interpolated
//!   `{$...}` fragment) has been scanned.
//!
//! Key details:
//! - PHP 8 lexes a whole qualified name as one token, longest match first, so a keyword inside
//!   one is never a keyword: `Default\Palette::accent()`, `catch (Self\Boom $e)`,
//!   `public static\Factory $f` and `use Function\Registry;` all name classes. elephc lexes the
//!   segments separately, so a keyword segment would reach every keyword arm of the parser
//!   (statement dispatch, `switch`/`match` labels, alternative-syntax terminators, member
//!   modifiers, `use function`). Rewriting it here gives every parser position the name.
//! - A word is a segment only when it touches the `\` in the source, and a leading word only
//!   when a further word touches that `\` too. `new \Foo`, `echo \strlen($s)` and
//!   `use function \Lib\f;` keep their keywords, and `Default\{A}` is not a name, as in PHP.
//! - A leading `namespace\` stays the `namespace` keyword: it is PHP's relative-name prefix
//!   (`namespace\Foo` is `Foo` in the current namespace), never a segment spelled `namespace`.
//!   After a separator (`\Demo\Namespace\Subject`) it is an ordinary segment.

use super::{SpannedToken, Token};

/// Rewrites every reserved-word token that is a segment of a glued qualified name into a
/// `Token::Identifier` carrying its source spelling. Identifiers and tokens that are not
/// words are left alone; spans and metadata are kept.
pub(super) fn identify_reserved_name_segments(tokens: &mut [SpannedToken]) {
    for index in 0..tokens.len() {
        if matches!(tokens[index].0, Token::Identifier(_))
            || is_constant_token(&tokens[index].0)
            || !is_word(tokens, index)
        {
            continue;
        }
        let after_separator = index > 0
            && matches!(tokens[index - 1].0, Token::Backslash)
            && tokens_touch(tokens, index - 1);
        let leads_name = !matches!(tokens[index].0, Token::Namespace)
            && matches!(tokens.get(index + 1), Some((Token::Backslash, _)))
            && tokens_touch(tokens, index)
            && is_word(tokens, index + 2)
            && tokens_touch(tokens, index + 1);
        if !(after_separator || leads_name) {
            continue;
        }
        let (token, metadata) = &tokens[index];
        let spelling = token
            .word_spelling(metadata)
            .expect("is_word checked the token has a word spelling")
            .to_string();
        tokens[index].0 = Token::Identifier(spelling);
    }
}

/// Returns whether `token` is a literal or predefined constant the lexer gives its own token
/// (`true`, `PHP_EOL`, `M_PI`, `STDERR`, ...). Those are values, not keywords: `\PHP_EOL` and
/// `\true` must keep their tokens so the parser still reads them as the global constant.
fn is_constant_token(token: &Token) -> bool {
    matches!(
        token,
        Token::True
            | Token::False
            | Token::Null
            | Token::PhpIntMax
            | Token::PhpIntMin
            | Token::PhpFloatMax
            | Token::PhpFloatMin
            | Token::PhpFloatEpsilon
            | Token::Inf
            | Token::Nan
            | Token::MPi
            | Token::ME
            | Token::MSqrt2
            | Token::MPi2
            | Token::MPi4
            | Token::MLog2e
            | Token::MLog10e
            | Token::Stdin
            | Token::Stdout
            | Token::Stderr
            | Token::PhpEol
            | Token::PhpOs
            | Token::DirectorySeparator
    )
}

/// Returns whether the token at `index` is spelled as a PHP label: an identifier, a keyword,
/// or a predefined constant the lexer gives its own token.
fn is_word(tokens: &[SpannedToken], index: usize) -> bool {
    tokens
        .get(index)
        .is_some_and(|(token, metadata)| token.word_spelling(metadata).is_some())
}

/// Returns whether the token at `left` ends exactly where the token after it starts, with no
/// whitespace or comment between them. Tokens without a source extent (the re-spanned tokens
/// of an interpolated fragment, which were already rewritten when the fragment was scanned)
/// never touch.
fn tokens_touch(tokens: &[SpannedToken], left: usize) -> bool {
    let (Some((_, first)), Some((_, second))) = (tokens.get(left), tokens.get(left + 1)) else {
        return false;
    };
    let (first, second) = (first.span, second.span);
    first.has_extent()
        && first.source_id() == second.source_id()
        && first.end_line == second.line
        && first.end_column() == second.col
}
