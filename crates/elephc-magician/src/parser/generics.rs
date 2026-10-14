//! Purpose:
//! Parses generic declarations, nested type arguments, and retained template syntax.
//!
//! Called from:
//! - Function/class/method declarations and type/name expression parsing.
//!
//! Key details:
//! - Splits right-shift tokens only in type lists; expression comparisons stay untouched.
//! - Type parameters keep their lexical spelling instead of namespace qualification.

use super::state::Parser;
use crate::errors::EvalParseError;
use crate::eval_ir::{generic_type, EvalGenericTemplate, EvalStmt};
use crate::lexer::{Token, TokenKind};
use elephc_generics::{Bindings, Parameter, Type};

impl Parser {
    /// Parses optional declared slots and enters their lexical type scope.
    pub(super) fn parse_generic_parameters(&mut self) -> Result<Vec<Parameter>, EvalParseError> {
        if !self.consume(TokenKind::Less) { return Ok(Vec::new()); }
        self.native_generic_syntax |= !self.token_phpdoc.get(self.pos.saturating_sub(1)).copied().unwrap_or(false);
        let mut params = Vec::new();
        loop {
            let word_variance = matches!(self.current(), TokenKind::Ident(name) if name == "out" || name == "in")
                && matches!(self.peek(), TokenKind::Ident(_));
            let variance = if word_variance {
                let variance = if matches!(self.current(), TokenKind::Ident(name) if name == "out") { 1 } else { -1 };
                self.advance();
                variance
            } else if self.consume(TokenKind::Plus) { 1 } else if self.consume(TokenKind::Minus) { -1 } else { 0 };
            let TokenKind::Ident(name) = self.current() else { return Err(EvalParseError::UnexpectedToken); };
            let name = name.clone();
            if params.iter().any(|p: &Parameter| p.name == name) {
                return Err(EvalParseError::UnsupportedConstruct);
            }
            self.type_parameters.push(name.clone());
            self.type_parameter_origins.push(self.pos);
            self.advance();
            let bound = if self.consume(TokenKind::Colon) { Some(self.parse_generic_type()?) } else { None };
            let default = if self.consume(TokenKind::Equal) { Some(self.parse_generic_type()?) } else { None };
            params.push(Parameter { name, bound, default, variance });
            if !self.consume(TokenKind::Comma) { break; }
        }
        self.expect_type_close()?;
        Ok(params)
    }

    /// Parses a type using the PHP type parser, retaining structural generic atoms.
    pub(super) fn parse_generic_type(&mut self) -> Result<Type, EvalParseError> {
        let ty = self.parse_type_decl(super::statements::EvalTypePosition::MethodReturn)?
            .ok_or(EvalParseError::UnexpectedToken)?;
        Ok(generic_type(&ty))
    }

    /// Parses a nonempty positional type argument list.
    pub(super) fn parse_type_arguments(&mut self) -> Result<Vec<Type>, EvalParseError> {
        self.expect(TokenKind::Less)?;
        self.native_generic_syntax |= !self.token_phpdoc.get(self.pos.saturating_sub(1)).copied().unwrap_or(false);
        let mut args = Vec::new();
        loop {
            args.push(self.parse_generic_type()?);
            if !self.consume(TokenKind::Comma) { break; }
        }
        self.expect_type_close()?;
        Ok(args)
    }

    /// Consumes one generic close, splitting PHP's right-shift token when necessary.
    fn expect_type_close(&mut self) -> Result<(), EvalParseError> {
        if matches!(self.current(), TokenKind::GreaterGreater) {
            self.tokens[self.pos] = TokenKind::Greater;
            self.tokens.insert(self.pos + 1, TokenKind::Greater);
            self.token_lines.insert(self.pos + 1, self.current_line());
            self.token_phpdoc.insert(self.pos + 1, self.token_phpdoc[self.pos]);
        }
        self.expect(TokenKind::Greater)
    }

    /// Claims only complete type lists, requiring a call or static separator outside class targets.
    pub(super) fn parse_written_name(&mut self, name: String, always: bool) -> Result<String, EvalParseError> {
        if !matches!(self.current(), TokenKind::Less) { return Ok(name); }
        let mut probe = self.clone();
        if probe.parse_type_arguments().is_err()
            || !always && !matches!(probe.current(), TokenKind::LParen | TokenKind::DoubleColon)
        {
            return Ok(name);
        }
        let args = self.parse_type_arguments()?;
        Ok(Type::Apply(name, args).to_string())
    }

    /// Marks only identifiers used in type or class-reference positions for substitution.
    pub(super) fn mark_type_parameter_use(&mut self, index: usize, name: &str) {
        if let Some(slot) = self.type_parameters.iter().rposition(|parameter| parameter == name) {
            self.type_parameter_uses.insert(index, self.type_parameter_origins[slot]);
        }
    }

    /// Retains a declaration after removing its generic header from the token stream.
    pub(super) fn retain_template(
        &self, start: usize, name_index: usize, header_start: usize, header_end: usize,
        name: String, parameters: Vec<Parameter>, prototype: EvalStmt,
    ) -> EvalGenericTemplate {
        let constructor = match &prototype {
            EvalStmt::ClassDecl(class) => class.method("__construct").and_then(|method| method.generic_template()),
            _ => None,
        };
        let constructor_parameters = constructor.map(|constructor| constructor.parameters.iter().map(|param| {
            let aliases = constructor.parameters.iter().map(|param| (param.name.clone(), Type::atom(format!("@ctor_{}", param.name)))).collect();
            Parameter { name: format!("@ctor_{}", param.name), bound: param.bound.as_ref().map(|ty| ty.substitute(&aliases)),
                default: param.default.as_ref().map(|ty| ty.substitute(&aliases)), variance: param.variance }
        }).collect()).unwrap_or_default();
        let native_syntax = !parameters.is_empty() && !self.token_phpdoc.get(header_start).copied().unwrap_or(false)
            || constructor.is_some_and(|constructor| constructor.native_syntax);
        let mut substitutions = Vec::new();
        let tokens = (start..self.pos).filter(|index| (*index < header_start || *index >= header_end)
            && !constructor.is_some_and(|constructor| *index >= constructor.header_range.0 && *index < constructor.header_range.1))
            .enumerate().map(|(relative, index)| {
                if let Some(origin) = self.type_parameter_uses.get(&index) {
                    if constructor.filter(|constructor| *origin >= constructor.header_range.0 && *origin < constructor.header_range.1).is_some() {
                        if let TokenKind::Ident(name) = &self.tokens[index] { substitutions.push((relative, format!("@ctor_{name}"))); }
                    } else if *origin >= header_start && *origin < header_end {
                        if let TokenKind::Ident(name) = &self.tokens[index] { substitutions.push((relative, name.clone())); }
                    }
                }
                Token::new(self.tokens[index].clone(), self.token_lines[index]).with_phpdoc(self.token_phpdoc[index])
            }).collect();
        EvalGenericTemplate { name, parameters, substitutions, prototype: Box::new(prototype), tokens,
            header_range: (header_start, header_end), constructor_parameters,
            namespace: self.namespace.clone(), imports: self.imports.clone(), name_index: name_index - start,
            native_syntax }

    }
}

/// Re-parses one specialization with concrete type syntax at the retained lexical scope.
pub(crate) fn instantiate(template: &EvalGenericTemplate, bindings: &Bindings, name: &str) -> Result<EvalStmt, EvalParseError> {
    let mut parser = specialization_parser(template, bindings, name)?;
    let program = parser.parse_program()?;
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
    let mut parser = Parser::new(crate::lexer::tokenize(source)?, source.len());
    let mut ty = parser.parse_generic_type()?;
    while matches!(parser.current(), TokenKind::Less) {
        ty = Type::Apply(ty.to_string(), parser.parse_type_arguments()?);
    }
    if !matches!(parser.current(), TokenKind::Eof) { return Err(EvalParseError::UnexpectedToken); }
    Ok(ty)
}

/// Reconstructs a parser at a retained declaration's lexical scope.
fn specialization_parser(template: &EvalGenericTemplate, bindings: &Bindings, name: &str) -> Result<Parser, EvalParseError> {
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
    let mut parser = Parser::new(tokens, 0);
    parser.namespace = template.namespace.clone();
    parser.imports = template.imports.clone();
    Ok(parser)
}

/// Replays a method template using the class grammar, preserving visibility and promotion rules.
pub(crate) fn instantiate_method(template: &EvalGenericTemplate, bindings: &Bindings, name: &str, method: &crate::eval_ir::EvalClassMethod) -> Result<crate::eval_ir::EvalClassMethod, EvalParseError> {
    let mut parser = specialization_parser(template, bindings, name)?;
    let (method, _) = parser.parse_class_method_decl(method.visibility(), method.is_static(), method.is_abstract(), method.is_final())?;
    Ok(method)
}
