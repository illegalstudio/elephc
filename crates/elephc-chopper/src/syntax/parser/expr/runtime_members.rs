//! Purpose:
//! Parses computed member accesses retained in the runtime syntax tree.
//!
//! Called from:
//! - The same prefix and postfix paths used by the compiler parser.
//!
//! Key details:
//! - A variable member followed by arguments names a method. Without arguments it is a property.
//! - Braces distinguish a computed constant name after :: from a computed property after ::$.

use crate::syntax::{
    errors::CompileError,
    lexer::{SpannedToken, Token},
    names::Name,
    parser::ast::{Expr, ExprKind, MemberArguments, RuntimeClassMember, RuntimeClassReceiver, RuntimeExpr, RuntimeWrite, TypeExpr},
    span::Span,
};

use super::{calls::parse_first_class_callable_parens, parse_args, parse_expr};

/// Retains a compound write or increment that yields a value at runtime.
pub(super) fn property_update(target: Expr, operation: RuntimeWrite, span: Span) -> Expr {
    Expr::new(ExprKind::Runtime(Box::new(RuntimeExpr::Write { target, operation })), span)
}

/// Retains written function type arguments for resolution in the runtime lexical scope.
pub(super) fn parse_generic_call(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    name: Name,
    type_args: Vec<TypeExpr>,
) -> Result<Expr, CompileError> {
    let arguments = parse_member_arguments(tokens, pos, span)?;
    Ok(Expr::new(ExprKind::Runtime(Box::new(RuntimeExpr::GenericFunction {
        name, type_args, arguments,
    })), span))
}

/// Reads a complete generic method suffix with a following invocation.
pub(super) fn parse_generic_method_arguments(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<(Vec<TypeExpr>, MemberArguments)>, CompileError> {
    let Some(after) = crate::syntax::parser::stmt::type_arguments_end(tokens, *pos) else {
        return Ok(None);
    };
    if !at_call(tokens, after) {
        return Ok(None);
    }
    let type_args = crate::syntax::parser::stmt::parse_type_arguments_only(tokens, pos, span)?;
    let arguments = parse_member_arguments(tokens, pos, span)?;
    Ok(Some((type_args, arguments)))
}

/// Keeps a spelled static method name separate from its type and call arguments.
pub(super) fn try_parse_static_generic_method(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    receiver: &crate::syntax::parser::ast::StaticReceiver,
    method: &str,
) -> Result<Option<Expr>, CompileError> {
    Ok(parse_generic_method_arguments(tokens, pos, span)?.map(|(type_args, arguments)| {
        Expr::new(ExprKind::Runtime(Box::new(RuntimeExpr::StaticMember {
            class: RuntimeClassReceiver::Named(receiver.clone()),
            member: RuntimeClassMember::GenericMethod {
                method: method.to_owned(), type_args, arguments,
            },
        })), span)
    }))
}

/// Identifies the named-class accesses that need source-level computed member syntax.
pub(super) fn has_computed_name(tokens: &[SpannedToken], pos: usize) -> bool {
    match tokens.get(pos).map(|(token, _)| token) {
        Some(Token::LBrace | Token::DollarBrace) => true,
        Some(Token::Variable(_)) => at_call(tokens, pos + 1),
        _ => false,
    }
}

/// Converts a parsed computed object member and its following arguments to source-level syntax.
pub(super) fn parse_object_call(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    object: Expr,
    member: Expr,
    nullsafe: bool,
) -> Result<Expr, CompileError> {
    let arguments = parse_member_arguments(tokens, pos, span)?;
    if nullsafe && matches!(arguments, MemberArguments::Callable) {
        return Err(CompileError::runtime_unsupported(span, "Cannot create a callable with a nullsafe operator"));
    }
    Ok(Expr::new(
        ExprKind::Runtime(Box::new(RuntimeExpr::ObjectMemberCall {
            object, member, arguments, nullsafe,
        })),
        span,
    ))
}

/// Parses the member following ::, keeping selection and invocation separate.
pub(super) fn parse_static_member(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    class: RuntimeClassReceiver,
) -> Result<Expr, CompileError> {
    let Some((token, metadata)) = tokens.get(*pos) else {
        return Err(CompileError::new(span, "Expected member after '::'"));
    };
    let member_span = metadata.span;
    *pos += 1;
    let member = match token {
        Token::DollarBrace => {
            RuntimeClassMember::ComputedProperty(parse_braced_name(tokens, pos, span)?)
        }
        Token::LBrace => {
            let name = parse_braced_name(tokens, pos, span)?;
            if at_call(tokens, *pos) {
                RuntimeClassMember::Method { name, arguments: parse_member_arguments(tokens, pos, span)? }
            } else {
                RuntimeClassMember::ComputedConstant(name)
            }
        }
        Token::Variable(name) => {
            if at_call(tokens, *pos) {
                let name = Expr::new(ExprKind::Variable(name.clone()), member_span);
                RuntimeClassMember::Method { name, arguments: parse_member_arguments(tokens, pos, span)? }
            } else {
                RuntimeClassMember::Property(name.clone())
            }
        }
        token => {
            let name = crate::syntax::parser::keyword_name::bareword_name_from_token(token, metadata)
                .ok_or_else(|| CompileError::new(member_span, "Expected member after '::'"))?;
            if let Some((type_args, arguments)) = parse_generic_method_arguments(tokens, pos, span)? {
                RuntimeClassMember::GenericMethod { method: name, type_args, arguments }
            } else if at_call(tokens, *pos) {
                let name = Expr::new(ExprKind::StringLiteral(name), member_span);
                RuntimeClassMember::Method { name, arguments: parse_member_arguments(tokens, pos, span)? }
            } else if name.eq_ignore_ascii_case("class") {
                RuntimeClassMember::ClassName
            } else {
                RuntimeClassMember::Constant(name)
            }
        }
    };
    Ok(Expr::new(ExprKind::Runtime(Box::new(RuntimeExpr::StaticMember { class, member })), span))
}

/// Parses a member name expression up to its closing brace.
fn parse_braced_name(tokens: &[SpannedToken], pos: &mut usize, span: Span) -> Result<Expr, CompileError> {
    let name = parse_expr(tokens, pos)?;
    if !matches!(tokens.get(*pos), Some((Token::RBrace, _))) {
        return Err(CompileError::new(span, "Expected '}' after member name"));
    }
    *pos += 1;
    Ok(name)
}

/// Returns whether the next token begins member invocation syntax.
fn at_call(tokens: &[SpannedToken], pos: usize) -> bool {
    matches!(tokens.get(pos), Some((Token::LParen, _)))
}

/// Parses argument lists and first-class callable markers after a member name.
fn parse_member_arguments(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<MemberArguments, CompileError> {
    *pos += 1;
    if parse_first_class_callable_parens(tokens, pos)? {
        Ok(MemberArguments::Callable)
    } else {
        parse_args(tokens, pos, span).map(MemberArguments::Call)
    }
}
