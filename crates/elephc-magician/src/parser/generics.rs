//! Purpose:
//! Parses generic declarations, nested type arguments, and retained template syntax.
//!
//! Called from:
//! - Function/class/method declarations and type/name expression parsing.
//!
//! Key details:
//! - Splits right-shift tokens only in type lists; expression comparisons stay untouched.
//! - Type parameters keep their lexical spelling instead of namespace qualification.

use super::state::FragmentInput;
use crate::errors::EvalParseError;
use crate::eval_ir::{EvalGenericTemplate, EvalStmt};
use crate::lexer::{Token, TokenKind};
use elephc_generics::{Bindings, Type};

/// Re-parses one specialization with concrete type syntax at the retained lexical scope.
pub(crate) fn instantiate(template: &EvalGenericTemplate, bindings: &Bindings, name: &str) -> Result<EvalStmt, EvalParseError> {
    let parser = specialization_parser(template, bindings, name)?;
    let program = super::with_parser_stack(|| super::lower_shared_program(&parser))
        .map_err(|failure| failure.eval_error(&parser.tokens))?;
    let mut declaration = program.statements().first().cloned().ok_or(EvalParseError::UnexpectedEof)?;
    match (&mut declaration, template.prototype.as_ref()) {
        (EvalStmt::FunctionDecl { attributes, .. }, EvalStmt::FunctionDecl { attributes: original, .. }) => *attributes = original.clone(),
        (EvalStmt::ClassDecl(class), EvalStmt::ClassDecl(original)) => *class = class.clone().with_attributes(original.attributes().to_vec()),
        (EvalStmt::InterfaceDecl(interface), EvalStmt::InterfaceDecl(original)) => *interface = interface.clone().with_attributes(original.attributes().to_vec()),
        (EvalStmt::TraitDecl(trait_decl), EvalStmt::TraitDecl(original)) => *trait_decl = trait_decl.clone().with_attributes(original.attributes().to_vec()),
        _ => {}
    }
    Ok(declaration)
}

/// Makes bound class names absolute so replay never qualifies them in the template namespace.
fn absolute_type(ty: &Type) -> Type {
    match ty {
        Type::Atom(name) if !["int", "float", "string", "bool", "false", "null", "void", "never", "mixed", "array", "callable", "iterable", "object", "self", "static", "parent"].contains(&name.as_str()) => Type::atom(format!("\\{}", name.trim_start_matches('\\'))),
        Type::Apply(name, args) => Type::Apply(if name == "array" { name.clone() } else { format!("\\{}", name.trim_start_matches('\\')) }, args.iter().map(absolute_type).collect()),
        Type::Nullable(inner) => Type::Nullable(Box::new(absolute_type(inner))),
        Type::Union(types) => Type::Union(types.iter().map(absolute_type).collect()),
        Type::Intersection(types) => Type::Intersection(types.iter().map(absolute_type).collect()),
        Type::Callable(params, ret) => Type::Callable(params.iter().map(absolute_type).collect(), Box::new(absolute_type(ret))),
        _ => ty.clone(),
    }
}

/// Decodes canonical type syntax, including specialization names recovered from objects.
pub(crate) fn parse_type_string(source: &str) -> Result<Type, EvalParseError> {
    let tokens = crate::lexer::tokenize(source)?;
    crate::lower::try_lower_type(&tokens).ok_or(EvalParseError::UnsupportedConstruct)
}

/// Reconstructs a parser at a retained declaration's lexical scope.
fn specialization_parser(template: &EvalGenericTemplate, bindings: &Bindings, name: &str) -> Result<FragmentInput, EvalParseError> {
    let mut tokens = Vec::new();
    for (index, token) in template.tokens.iter().enumerate() {
        if index == template.name_index {
            let prefix = format!("{}\\", template.namespace);
            let local_name = name.strip_prefix(&prefix).unwrap_or(name);
            tokens.push(Token::new(TokenKind::Ident(local_name.into()), token.line()));
            continue;
        }
        if let Some((_, identifier)) = template.substitutions.iter().find(|(position, _)| *position == index) {
            if let Some((_, ty)) = bindings.iter().find(|(slot, _)| slot == identifier) {
                let replacement = crate::lexer::tokenize(&absolute_type(ty).to_string())?;
                tokens.extend(replacement.into_iter().filter(|token| !matches!(token.kind(), TokenKind::Eof))
                    .map(|replacement| Token::new(replacement.into_kind(), token.line())));
                continue;
            }
        }
        tokens.push(token.clone());
    }
    tokens.push(Token::new(TokenKind::Eof, template.tokens.last().map_or(1, Token::line)));
    let mut parser = FragmentInput::new(tokens, 0);
    parser.namespace = template.namespace.clone();
    parser.imports = template.imports.clone();
    parser.strict_types = template.strict_types;
    Ok(parser)
}

/// Replays a method template using the class grammar, preserving visibility and promotion rules.
pub(crate) fn instantiate_method(template: &EvalGenericTemplate, bindings: &Bindings, name: &str, method: &crate::eval_ir::EvalClassMethod) -> Result<crate::eval_ir::EvalClassMethod, EvalParseError> {
    let parser = specialization_parser(template, bindings, name)?;
    super::with_parser_stack(|| lower_specialized_method(&parser, method))
        .ok_or(EvalParseError::UnsupportedConstruct)
}

/// Reuses Chopper's class member grammar for the body of a specialized method.
fn lower_specialized_method(
    parser: &FragmentInput,
    method: &crate::eval_ir::EvalClassMethod,
) -> Option<crate::eval_ir::EvalClassMethod> {
    use crate::eval_ir::EvalVisibility;

    let line = *parser.token_lines.first()?;
    let mut source = Vec::new();
    let keyword = |name: &str| Token::new(TokenKind::Ident(name.to_owned()), line);
    if method.is_abstract() {
        source.push(keyword("abstract"));
    }
    source.extend([
        keyword("class"),
        keyword("__elephc_runtime_method"),
        Token::new(TokenKind::LBrace, line),
        keyword(match method.visibility() {
            EvalVisibility::Public => "public",
            EvalVisibility::Protected => "protected",
            EvalVisibility::Private => "private",
        }),
    ]);
    if method.is_abstract() {
        source.push(keyword("abstract"));
    }
    if method.is_final() {
        source.push(keyword("final"));
    }
    if method.is_static() {
        source.push(keyword("static"));
    }
    source.extend(parser.tokens.iter().zip(&parser.token_lines).zip(&parser.token_phpdoc)
        .filter(|((kind, _), _)| **kind != TokenKind::Eof)
        .map(|((kind, line), phpdoc)| Token::new(kind.clone(), *line).with_phpdoc(*phpdoc)));
    let end = *parser.token_lines.last()?;
    source.extend([Token::new(TokenKind::RBrace, end), Token::new(TokenKind::Eof, end)]);
    let mut wrapped = FragmentInput::new(source, parser.source_len);
    wrapped.namespace = parser.namespace.clone();
    wrapped.imports = parser.imports.clone();
    wrapped.strict_types = parser.strict_types;
    let program = super::lower_shared_program(&wrapped).ok()?;
    let [EvalStmt::ClassDecl(class)] = program.statements() else { return None };
    let [method] = class.methods() else { return None };
    Some(method.clone())
}
