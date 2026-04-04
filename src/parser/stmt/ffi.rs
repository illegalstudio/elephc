use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{CType, ExternField, ExternParam, Stmt, StmtKind};
use crate::span::Span;

use super::{expect_semicolon, expect_token, recover_to_statement_boundary};

fn parse_c_type(tokens: &[(Token, Span)], pos: &mut usize) -> Result<CType, CompileError> {
    let span = if *pos < tokens.len() {
        tokens[*pos].1
    } else {
        Span::dummy()
    };
    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(name)) => {
            let name = name.clone();
            *pos += 1;
            match name.as_str() {
                "int" | "integer" => Ok(CType::Int),
                "float" | "double" => Ok(CType::Float),
                "string" => Ok(CType::Str),
                "bool" | "boolean" => Ok(CType::Bool),
                "void" => Ok(CType::Void),
                "callable" => Ok(CType::Callable),
                "ptr" => {
                    // Check for ptr<TypeName>
                    if *pos < tokens.len() && tokens[*pos].0 == Token::Less {
                        *pos += 1; // consume <
                        let type_name = match tokens.get(*pos).map(|(t, _)| t) {
                            Some(Token::Identifier(t)) => {
                                let t = t.clone();
                                *pos += 1;
                                t
                            }
                            _ => {
                                return Err(CompileError::new(
                                    span,
                                    "Expected type name after 'ptr<'",
                                ))
                            }
                        };
                        if *pos >= tokens.len() || tokens[*pos].0 != Token::Greater {
                            return Err(CompileError::new(span, "Expected '>' after ptr<T"));
                        }
                        *pos += 1; // consume >
                        Ok(CType::TypedPtr(type_name))
                    } else {
                        Ok(CType::Ptr)
                    }
                }
                _ => Err(CompileError::new(
                    span,
                    &format!("Unknown C type: {}", name),
                )),
            }
        }
        _ => Err(CompileError::new(span, "Expected type name")),
    }
}

fn parse_extern_params(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Vec<ExternParam>, CompileError> {
    let mut params = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Expected ',' between extern parameters",
                ));
            }
            *pos += 1;
        }
        let c_type = parse_c_type(tokens, pos)?;
        let name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                let n = n.clone();
                *pos += 1;
                n
            }
            _ => {
                return Err(CompileError::new(
                    if *pos < tokens.len() {
                        tokens[*pos].1
                    } else {
                        Span::dummy()
                    },
                    "Expected $parameter_name after type",
                ))
            }
        };
        params.push(ExternParam { name, c_type });
    }
    Ok(params)
}

fn parse_extern_function(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    library: Option<String>,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'function'
    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => {
            return Err(CompileError::new(
                span,
                "Expected function name after 'extern function'",
            ))
        }
    };
    expect_token(
        tokens,
        pos,
        &Token::LParen,
        "Expected '(' after extern function name",
    )?;
    let params = parse_extern_params(tokens, pos)?;
    expect_token(
        tokens,
        pos,
        &Token::RParen,
        "Expected ')' after extern parameters",
    )?;

    // Parse return type: ': type'
    let return_type = if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        parse_c_type(tokens, pos)?
    } else {
        CType::Void
    };

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        },
        span,
    ))
}

/// Parse extern declarations. Returns Vec<Stmt> because extern "lib" { } blocks produce multiple stmts.
/// Called from parse() in mod.rs, not from parse_stmt.
pub fn parse_extern_stmts(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Vec<Stmt>, CompileError> {
    let span = tokens[*pos].1;
    *pos += 1; // consume 'extern'

    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Function) => Ok(vec![parse_extern_function(tokens, pos, span, None)?]),

        Some(Token::StringLiteral(lib)) => {
            let library = lib.clone();
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == Token::Function {
                // extern "lib" function name(): type;
                return Ok(vec![parse_extern_function(
                    tokens,
                    pos,
                    span,
                    Some(library),
                )?]);
            }
            // extern "lib" { function ...; function ...; }
            expect_token(
                tokens,
                pos,
                &Token::LBrace,
                "Expected '{' or 'function' after extern library name",
            )?;
            let mut stmts = Vec::new();
            let mut errors = Vec::new();
            while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
                if tokens[*pos].0 != Token::Function {
                    errors.push(CompileError::new(
                        tokens[*pos].1,
                        "Expected 'function' inside extern block",
                    ));
                    recover_to_statement_boundary(tokens, pos);
                    continue;
                }
                match parse_extern_function(
                    tokens,
                    pos,
                    span,
                    Some(library.clone()),
                ) {
                    Ok(stmt) => stmts.push(stmt),
                    Err(error) => {
                        errors.extend(error.flatten());
                        recover_to_statement_boundary(tokens, pos);
                    }
                }
            }
            expect_token(
                tokens,
                pos,
                &Token::RBrace,
                "Expected '}' after extern block",
            )?;
            if stmts.is_empty() {
                errors.push(CompileError::new(span, "Empty extern block"));
            }
            if errors.is_empty() {
                Ok(stmts)
            } else {
                Err(CompileError::from_many(errors))
            }
        }

        Some(Token::Class) => {
            *pos += 1; // consume 'class'
            let name = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Identifier(n)) => {
                    let n = n.clone();
                    *pos += 1;
                    n
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "Expected class name after 'extern class'",
                    ))
                }
            };
            expect_token(
                tokens,
                pos,
                &Token::LBrace,
                "Expected '{' after extern class name",
            )?;
            let mut fields = Vec::new();
            while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
                if tokens[*pos].0 == Token::Public {
                    *pos += 1;
                }
                let c_type = parse_c_type(tokens, pos)?;
                let field_name = match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Variable(n)) => {
                        let n = n.clone();
                        *pos += 1;
                        n
                    }
                    _ => {
                        return Err(CompileError::new(
                            if *pos < tokens.len() {
                                tokens[*pos].1
                            } else {
                                Span::dummy()
                            },
                            "Expected $field_name in extern class",
                        ))
                    }
                };
                expect_semicolon(tokens, pos)?;
                fields.push(ExternField {
                    name: field_name,
                    c_type,
                });
            }
            expect_token(
                tokens,
                pos,
                &Token::RBrace,
                "Expected '}' after extern class body",
            )?;
            Ok(vec![Stmt::new(
                StmtKind::ExternClassDecl { name, fields },
                span,
            )])
        }

        Some(Token::Global) => {
            *pos += 1; // consume 'global'
            let c_type = parse_c_type(tokens, pos)?;
            let name = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Variable(n)) => {
                    let n = n.clone();
                    *pos += 1;
                    n
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "Expected $variable_name after extern global type",
                    ))
                }
            };
            expect_semicolon(tokens, pos)?;
            Ok(vec![Stmt::new(
                StmtKind::ExternGlobalDecl { name, c_type },
                span,
            )])
        }

        _ => Err(CompileError::new(
            span,
            "Expected 'function', string literal, 'class', or 'global' after 'extern'",
        )),
    }
}
