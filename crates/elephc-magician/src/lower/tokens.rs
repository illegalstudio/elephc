//! Purpose:
//! Adapts the runtime eval tokens whose grammar is shared by Chopper.
//!
//! Called from:
//! - The Magician AST-to-EvalIR lowering entry point.
//!
//! Key details:
//! - Token spelling and source lines remain available to the parser.
//! - The adapter refuses token forms outside the runtime grammar.

use elephc_chopper::syntax::{
    lexer::{SpannedToken, Token, TokenMetadata, token::spanned},
    span::Span,
};

use crate::{eval_ir::EvalMagicConst, lexer::TokenKind};

/// Converts an eligible runtime token stream to Chopper parser input.
pub(super) fn adapt_fragment(tokens: &[TokenKind], lines: &[i64]) -> Option<Vec<SpannedToken>> {
    if tokens.len() != lines.len() {
        return None;
    }
    let mut input = Vec::with_capacity(tokens.len());
    for (index, (kind, line)) in tokens.iter().zip(lines).enumerate() {
        let token = shared_token(kind, index.checked_sub(1).map(|index| &tokens[index]), tokens.get(index + 1))?;
        // In a braced interpolation the expression's line token retains its own
        // fragment-relative value even though token metadata belongs to the outer string.
        let line = match kind {
            TokenKind::Magic(EvalMagicConst::Line(line)) => *line,
            _ => *line,
        };
        // Token ordinals distinguish source order when several labels occupy one line.
        let span = Span::new(u32::try_from(line).ok()?, u32::try_from(index).ok()?.checked_add(1)?);
        input.push(match kind {
            TokenKind::Ident(text) if !matches!(token, Token::Identifier(_)) => {
                (token, TokenMetadata::with_source_spelling(span, text))
            }
            _ => spanned(token, span),
        });
    }
    Some(input)
}

/// Maps the eval lexer tokens whose syntax and AST lowering are shared by both parsers.
fn shared_token(kind: &TokenKind, previous: Option<&TokenKind>, next: Option<&TokenKind>) -> Option<Token> {
    Some(match kind {
        TokenKind::Ident(name)
            if matches!(previous, Some(TokenKind::Arrow | TokenKind::QuestionArrow | TokenKind::Backslash)) =>
        {
            Token::Identifier(name.clone())
        }
        TokenKind::Ident(name)
            if matches!(previous, Some(TokenKind::DoubleColon))
                && (!name.eq_ignore_ascii_case("class") || matches!(next, Some(TokenKind::LParen))) =>
        {
            Token::Identifier(name.clone())
        }
        TokenKind::Ident(name)
            if name.eq_ignore_ascii_case("self") && relative_class_receiver(previous, next) => Token::Self_,
        TokenKind::Ident(name)
            if name.eq_ignore_ascii_case("parent") && relative_class_receiver(previous, next) => Token::Parent,
        TokenKind::Ident(name)
            if matches!(previous, Some(TokenKind::Ident(keyword)) if keyword.eq_ignore_ascii_case("function"))
                && !(name.eq_ignore_ascii_case("namespace") && matches!(next, Some(TokenKind::Backslash))) =>
        {
            Token::Identifier(name.clone())
        }
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("class") => Token::Class,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("enum") => Token::Enum,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("interface") => Token::Interface,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("trait") => Token::Trait,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("insteadof") => Token::InsteadOf,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("var") => Token::Identifier(name.clone()),
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("extends") => Token::Extends,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("implements") => Token::Implements,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("public") => Token::Public,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("protected") => Token::Protected,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("private") => Token::Private,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("abstract") => Token::Abstract,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("readonly") => Token::ReadOnly,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("static") => Token::Static,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("namespace") => Token::Namespace,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("declare") => Token::Declare,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("use") => Token::Use,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("const") => Token::Const,
        TokenKind::ReservedNameHead(name) => Token::Identifier(name.clone()),
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("final") => Token::Final,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("return") => Token::Return,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("echo") => Token::Echo,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("print") => Token::Print,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("clone") => Token::Clone,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("instanceof") => Token::InstanceOf,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("if") => Token::If,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("elseif") => Token::ElseIf,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("else") => Token::Else,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("while") => Token::While,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("do") => Token::Do,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("for") => Token::For,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("foreach") => Token::Foreach,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("as") => Token::As,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("global") => Token::Global,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("include") => Token::Include,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("include_once") => Token::IncludeOnce,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("require") => Token::Require,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("require_once") => Token::RequireOnce,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("function") => Token::Function,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("new") => Token::New,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("goto") => Token::Goto,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("match") => Token::Match,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("switch") => Token::Switch,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("case") => Token::Case,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("endswitch") => Token::EndSwitch,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("endif") => Token::EndIf,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("endfor") => Token::EndFor,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("endwhile") => Token::EndWhile,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("endforeach") => Token::EndForeach,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("default") => Token::Default,
        TokenKind::Ident(name) if ["eval", "isset", "empty", "unset", "exit", "die"]
            .iter().any(|keyword| name.eq_ignore_ascii_case(keyword)) => Token::Identifier(name.clone()),
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("throw") => Token::Throw,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("try") => Token::Try,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("catch") => Token::Catch,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("finally") => Token::Finally,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("break") => Token::Break,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("continue") => Token::Continue,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("and") => Token::And,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("or") => Token::Or,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("xor") => Token::Xor,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("true") => Token::True,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("false") => Token::False,
        TokenKind::Ident(name) if name.eq_ignore_ascii_case("null") => Token::Null,
        TokenKind::Ident(name) if is_shared_identifier(name) || is_shared_type_word(name) => {
            Token::Identifier(name.clone())
        }
        TokenKind::Int(value) => Token::IntLiteral(*value),
        TokenKind::Float(value) => Token::FloatLiteral(*value),
        TokenKind::Magic(magic) => match magic {
            EvalMagicConst::File => Token::DunderFile,
            EvalMagicConst::Dir => Token::DunderDir,
            EvalMagicConst::Function => Token::DunderFunction,
            EvalMagicConst::Class => Token::DunderClass,
            EvalMagicConst::Method => Token::DunderMethod,
            EvalMagicConst::Namespace => Token::DunderNamespace,
            EvalMagicConst::Trait => Token::DunderTrait,
            EvalMagicConst::Line(_) => Token::DunderLine,
        },
        TokenKind::String(value) => Token::StringLiteral(value.clone()),
        TokenKind::DollarIdent(name) => Token::Variable(name.clone()),
        TokenKind::DollarLBrace if matches!(previous, Some(TokenKind::DoubleColon)) => Token::DollarBrace,
        TokenKind::Plus => Token::Plus,
        TokenKind::Minus => Token::Minus,
        TokenKind::Bang => Token::Bang,
        TokenKind::Tilde => Token::Tilde,
        TokenKind::Dot => Token::Dot,
        TokenKind::Star => Token::Star,
        TokenKind::StarStar => Token::StarStar,
        TokenKind::Slash => Token::Slash,
        TokenKind::Percent => Token::Percent,
        TokenKind::EqualEqual => Token::EqualEqual,
        TokenKind::EqualEqualEqual => Token::EqualEqualEqual,
        TokenKind::NotEqual => Token::NotEqual,
        TokenKind::NotEqualEqual => Token::NotEqualEqual,
        TokenKind::Less => Token::Less,
        TokenKind::LessEqual => Token::LessEqual,
        TokenKind::Greater => Token::Greater,
        TokenKind::GreaterEqual => Token::GreaterEqual,
        TokenKind::Spaceship => Token::Spaceship,
        TokenKind::LessLess => Token::LessLess,
        TokenKind::GreaterGreater => Token::GreaterGreater,
        TokenKind::Pipe => Token::Pipe,
        TokenKind::Caret => Token::Caret,
        TokenKind::Ampersand => Token::Ampersand,
        TokenKind::AndAnd => Token::AndAnd,
        TokenKind::OrOr => Token::OrOr,
        TokenKind::Arrow => Token::Arrow,
        TokenKind::QuestionArrow => Token::QuestionArrow,
        TokenKind::DoubleColon => Token::DoubleColon,
        TokenKind::QuestionQuestion => Token::QuestionQuestion,
        TokenKind::Question => Token::Question,
        TokenKind::Colon => Token::Colon,
        TokenKind::Equal => Token::Assign,
        TokenKind::PlusPlus => Token::PlusPlus,
        TokenKind::MinusMinus => Token::MinusMinus,
        TokenKind::PlusEqual => Token::PlusAssign,
        TokenKind::MinusEqual => Token::MinusAssign,
        TokenKind::StarEqual => Token::StarAssign,
        TokenKind::StarStarEqual => Token::StarStarAssign,
        TokenKind::SlashEqual => Token::SlashAssign,
        TokenKind::PercentEqual => Token::PercentAssign,
        TokenKind::DotEqual => Token::DotAssign,
        TokenKind::AmpEqual => Token::AmpAssign,
        TokenKind::PipeEqual => Token::PipeAssign,
        TokenKind::CaretEqual => Token::CaretAssign,
        TokenKind::LessLessEqual => Token::LessLessAssign,
        TokenKind::GreaterGreaterEqual => Token::GreaterGreaterAssign,
        TokenKind::FatArrow => Token::DoubleArrow,
        TokenKind::LParen => Token::LParen,
        TokenKind::RParen => Token::RParen,
        TokenKind::LBracket => Token::LBracket,
        TokenKind::RBracket => Token::RBracket,
        TokenKind::LBrace => Token::LBrace,
        TokenKind::RBrace => Token::RBrace,
        TokenKind::Ellipsis => Token::Ellipsis,
        TokenKind::Backslash => Token::Backslash,
        TokenKind::AttributeStart => Token::AttrOpen,
        TokenKind::Comma => Token::Comma,
        TokenKind::Semicolon => Token::Semicolon,
        TokenKind::Eof => Token::Eof,
        _ => return None,
    })
}

/// Identifies bare class references without rewriting a qualified segment.
fn relative_class_receiver(previous: Option<&TokenKind>, next: Option<&TokenKind>) -> bool {
    !matches!(previous, Some(TokenKind::Backslash))
        && (matches!(next, Some(TokenKind::DoubleColon))
            || matches!(previous, Some(TokenKind::Ident(word))
                if word.eq_ignore_ascii_case("new") || word.eq_ignore_ascii_case("instanceof")))
}

/// Checks for identifiers that do not have special eval grammar or cast meaning.
fn is_shared_identifier(name: &str) -> bool {
    !crate::parser::is_reserved_keyword(name)
        && !name.eq_ignore_ascii_case("enum")
}

/// Recognizes type words whose spelling the compiler and Magician interpret identically.
fn is_shared_type_word(name: &str) -> bool {
    ["int", "float", "bool", "string", "callable", "array"]
        .iter()
        .any(|ty| name.eq_ignore_ascii_case(ty))
}
