use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_expr;
use crate::span::Span;

pub fn parse_stmt(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Stmt, CompileError> {
    let span = tokens[*pos].1;

    match &tokens[*pos].0 {
        Token::Echo => parse_echo(tokens, pos, span),
        Token::Variable(_) => parse_variable_stmt(tokens, pos, span),
        Token::PlusPlus | Token::MinusMinus => parse_incdec_stmt(tokens, pos, span),
        Token::Function => parse_function_decl(tokens, pos, span),
        Token::Return => parse_return(tokens, pos, span),
        Token::Identifier(_) => {
            let expr = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
        }
        Token::If => parse_if(tokens, pos, span),
        Token::While => parse_while(tokens, pos, span),
        Token::Do => parse_do_while(tokens, pos, span),
        Token::Foreach => parse_foreach(tokens, pos, span),
        Token::For => parse_for(tokens, pos, span),
        Token::Break => {
            *pos += 1;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::Break, span))
        }
        Token::Continue => {
            *pos += 1;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::Continue, span))
        }
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token at statement position: {:?}", other),
        )),
    }
}

fn parse_echo(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Echo(expr), span))
}

/// Handle statements starting with $variable: assignment or post-increment/decrement.
fn parse_variable_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };

    // Check for array access: $var[...]
    if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::LBracket {
        let var_name = match &tokens[*pos].0 {
            Token::Variable(n) => n.clone(),
            _ => unreachable!(),
        };
        *pos += 1; // consume $var
        *pos += 1; // consume [

        // $var[] = ... (push)
        if *pos < tokens.len() && tokens[*pos].0 == Token::RBracket {
            *pos += 1; // consume ]
            expect_token(tokens, pos, &Token::Assign, "Expected '=' after '$var[]'")?;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            return Ok(Stmt::new(
                StmtKind::ArrayPush {
                    array: var_name,
                    value,
                },
                span,
            ));
        }

        // $var[index] = ... (assign)
        let index = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
            return Err(CompileError::new(span, "Expected ']'"));
        }
        *pos += 1; // consume ]

        if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            return Ok(Stmt::new(
                StmtKind::ArrayAssign {
                    array: var_name,
                    index,
                    value,
                },
                span,
            ));
        }

        return Err(CompileError::new(span, "Expected '=' after array access"));
    }

    // Peek at token after variable
    if *pos + 1 < tokens.len() {
        match &tokens[*pos + 1].0 {
            Token::PlusPlus => {
                *pos += 2; // consume $var and ++
                expect_semicolon(tokens, pos)?;
                let expr = Expr::new(ExprKind::PostIncrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            Token::MinusMinus => {
                *pos += 2; // consume $var and --
                expect_semicolon(tokens, pos)?;
                let expr = Expr::new(ExprKind::PostDecrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            _ => {}
        }
    }

    // Regular or compound assignment
    parse_assign(tokens, pos, span)
}

fn parse_assign(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };
    *pos += 1;

    if *pos >= tokens.len() {
        return Err(CompileError::new(span, "Expected '=' after variable name"));
    }

    // Check for compound assignment operators
    use crate::parser::ast::BinOp;
    let compound_op = match &tokens[*pos].0 {
        Token::PlusAssign => Some(BinOp::Add),
        Token::MinusAssign => Some(BinOp::Sub),
        Token::StarAssign => Some(BinOp::Mul),
        Token::SlashAssign => Some(BinOp::Div),
        Token::PercentAssign => Some(BinOp::Mod),
        Token::DotAssign => Some(BinOp::Concat),
        Token::Assign => None,
        _ => return Err(CompileError::new(span, "Expected '=' after variable name")),
    };
    *pos += 1;

    let rhs = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    let value = if let Some(op) = compound_op {
        // Desugar: $x += expr → $x = $x + expr
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::Variable(name.clone()), span)),
                op,
                right: Box::new(rhs),
            },
            span,
        )
    } else {
        rhs
    };

    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

/// Handle ++$var; or --$var; as standalone statements.
fn parse_incdec_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let is_increment = tokens[*pos].0 == Token::PlusPlus;
    *pos += 1;

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => {
            let op = if is_increment { "++" } else { "--" };
            return Err(CompileError::new(span, &format!("Expected variable after '{}'", op)));
        }
    };
    *pos += 1;
    expect_semicolon(tokens, pos)?;

    let kind = if is_increment {
        ExprKind::PreIncrement(name)
    } else {
        ExprKind::PreDecrement(name)
    };
    let expr = Expr::new(kind, span);
    Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
}

/// Parse: if (expr) { stmts } (elseif (expr) { stmts })* (else { stmts })?
fn parse_if(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'if'

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'if'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after if condition")?;
    let then_body = parse_block(tokens, pos)?;

    let mut elseif_clauses = Vec::new();
    let mut else_body = None;

    loop {
        if *pos >= tokens.len() {
            break;
        }
        if tokens[*pos].0 == Token::ElseIf {
            *pos += 1;
            expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'elseif'")?;
            let cond = parse_expr(tokens, pos)?;
            expect_token(tokens, pos, &Token::RParen, "Expected ')' after elseif condition")?;
            let body = parse_block(tokens, pos)?;
            elseif_clauses.push((cond, body));
        } else if tokens[*pos].0 == Token::Else {
            *pos += 1;
            else_body = Some(parse_block(tokens, pos)?);
            break;
        } else {
            break;
        }
    }

    Ok(Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        },
        span,
    ))
}

/// Parse: while (expr) { stmts }
fn parse_while(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'while'

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'while'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after while condition")?;
    let body = parse_block(tokens, pos)?;

    Ok(Stmt::new(StmtKind::While { condition, body }, span))
}

/// Parse: foreach ($array as $value) { stmts }
fn parse_foreach(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'foreach'

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'foreach'")?;
    let array = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::As, "Expected 'as' in foreach")?;

    let value_var = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable after 'as'")),
    };
    *pos += 1;

    expect_token(tokens, pos, &Token::RParen, "Expected ')' after foreach")?;
    let body = parse_block(tokens, pos)?;

    Ok(Stmt::new(
        StmtKind::Foreach {
            array,
            value_var,
            body,
        },
        span,
    ))
}

/// Parse: do { stmts } while (expr);
fn parse_do_while(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'do'

    let body = parse_block(tokens, pos)?;

    expect_token(tokens, pos, &Token::While, "Expected 'while' after do block")?;
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'while'")?;
    let condition = parse_expr(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after condition")?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::DoWhile { body, condition }, span))
}

/// Parse: for (init; condition; update) { stmts }
fn parse_for(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'for'

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after 'for'")?;

    // Init (optional assignment)
    let init = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        let init_span = tokens[*pos].1;
        let s = parse_assign_inline(tokens, pos, init_span)?;
        Some(Box::new(s))
    } else {
        None
    };
    expect_semicolon(tokens, pos)?;

    // Condition (optional expression)
    let condition = if *pos < tokens.len() && tokens[*pos].0 != Token::Semicolon {
        Some(parse_expr(tokens, pos)?)
    } else {
        None
    };
    expect_semicolon(tokens, pos)?;

    // Update (optional assignment)
    let update = if *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        let update_span = tokens[*pos].1;
        let s = parse_assign_inline(tokens, pos, update_span)?;
        Some(Box::new(s))
    } else {
        None
    };
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after for clauses")?;

    let body = parse_block(tokens, pos)?;

    Ok(Stmt::new(
        StmtKind::For {
            init,
            condition,
            update,
            body,
        },
        span,
    ))
}

/// Parse a simple statement without trailing semicolon (for use inside for-loops).
/// Handles: $var = expr, $var++, $var--, ++$var, --$var
fn parse_assign_inline(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    // Pre-increment/decrement
    if *pos < tokens.len() {
        match &tokens[*pos].0 {
            Token::PlusPlus => {
                *pos += 1;
                let name = match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Variable(n)) => n.clone(),
                    _ => return Err(CompileError::new(span, "Expected variable after '++'")),
                };
                *pos += 1;
                let expr = Expr::new(ExprKind::PreIncrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            Token::MinusMinus => {
                *pos += 1;
                let name = match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Variable(n)) => n.clone(),
                    _ => return Err(CompileError::new(span, "Expected variable after '--'")),
                };
                *pos += 1;
                let expr = Expr::new(ExprKind::PreDecrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            _ => {}
        }
    }

    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable in for clause")),
    };
    *pos += 1;

    // Post-increment/decrement
    if *pos < tokens.len() {
        match &tokens[*pos].0 {
            Token::PlusPlus => {
                *pos += 1;
                let expr = Expr::new(ExprKind::PostIncrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            Token::MinusMinus => {
                *pos += 1;
                let expr = Expr::new(ExprKind::PostDecrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            _ => {}
        }
    }

    // Regular assignment
    if *pos >= tokens.len() || tokens[*pos].0 != Token::Assign {
        return Err(CompileError::new(span, "Expected '=' after variable name"));
    }
    *pos += 1;

    let value = parse_expr(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

/// Parse: function name($param1, $param2) { stmts }
fn parse_function_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'function'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected function name")),
    };
    *pos += 1;

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after function name")?;

    let mut params = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() {
            expect_token(tokens, pos, &Token::Comma, "Expected ',' between parameters")?;
        }
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                params.push(n.clone());
                *pos += 1;
            }
            _ => return Err(CompileError::new(span, "Expected parameter variable")),
        }
    }
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after parameters")?;

    let body = parse_block(tokens, pos)?;

    Ok(Stmt::new(
        StmtKind::FunctionDecl { name, params, body },
        span,
    ))
}

/// Parse: return; or return expr;
fn parse_return(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'return'

    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        return Ok(Stmt::new(StmtKind::Return(None), span));
    }

    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Return(Some(expr)), span))
}

/// Parse a brace-delimited block: { stmt* }
pub fn parse_block(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Vec<Stmt>, CompileError> {
    let span = if *pos < tokens.len() {
        tokens[*pos].1
    } else {
        Span::dummy()
    };
    expect_token(tokens, pos, &Token::LBrace, "Expected '{'")?;

    let mut stmts = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        stmts.push(parse_stmt(tokens, pos)?);
    }

    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBrace {
        return Err(CompileError::new(span, "Expected '}'"));
    }
    *pos += 1;

    Ok(stmts)
}

fn expect_semicolon(tokens: &[(Token, Span)], pos: &mut usize) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() {
            tokens[*pos].1
        } else {
            Span::dummy()
        };
        Err(CompileError::new(span, "Expected ';'"))
    }
}

fn expect_token(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    expected: &Token,
    msg: &str,
) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == *expected {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() {
            tokens[*pos].1
        } else {
            Span::dummy()
        };
        Err(CompileError::new(span, msg))
    }
}
