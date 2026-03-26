use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, Visibility, ClassProperty, ClassMethod};
use crate::parser::control;
use crate::parser::expr::parse_expr;
use crate::span::Span;

pub fn parse_stmt(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Stmt, CompileError> {
    let span = tokens[*pos].1;

    match &tokens[*pos].0 {
        Token::Echo | Token::Print => parse_echo(tokens, pos, span),
        Token::Variable(_) => parse_variable_stmt(tokens, pos, span),
        Token::This => parse_this_stmt(tokens, pos, span),
        Token::PlusPlus | Token::MinusMinus => parse_incdec_stmt(tokens, pos, span),
        Token::Class => parse_class_decl(tokens, pos, span),
        Token::Function => parse_function_decl(tokens, pos, span),
        Token::Return => parse_return(tokens, pos, span),
        Token::Include => parse_include(tokens, pos, span, false, false),
        Token::IncludeOnce => parse_include(tokens, pos, span, true, false),
        Token::Require => parse_include(tokens, pos, span, false, true),
        Token::RequireOnce => parse_include(tokens, pos, span, true, true),
        Token::Const => parse_const_decl(tokens, pos, span),
        Token::Global => parse_global(tokens, pos, span),
        Token::Static => parse_static_var(tokens, pos, span),
        Token::LBracket => parse_list_unpack(tokens, pos, span),
        Token::Identifier(_) => {
            let expr = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
        }
        // Control flow — delegated to control.rs
        Token::Switch => control::parse_switch(tokens, pos, span),
        Token::If => control::parse_if(tokens, pos, span),
        Token::While => control::parse_while(tokens, pos, span),
        Token::Do => control::parse_do_while(tokens, pos, span),
        Token::For => control::parse_for(tokens, pos, span),
        Token::Foreach => control::parse_foreach(tokens, pos, span),
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

fn parse_include(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    once: bool,
    required: bool,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume include/require keyword

    // Support both: include 'file.php'; and include('file.php');
    let has_parens = *pos < tokens.len() && tokens[*pos].0 == Token::LParen;
    if has_parens {
        *pos += 1;
    }

    let path = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::StringLiteral(s)) => s.clone(),
        _ => return Err(CompileError::new(span, "Expected string path after include/require")),
    };
    *pos += 1;

    if has_parens {
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(span, "Expected ')' after include path"));
        }
        *pos += 1;
    }

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Include { path, once, required }, span))
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

/// Handle statements starting with $variable: assignment, array ops, or post-increment.
fn parse_variable_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };

    // Property access/method call: $var->prop or $var->method()
    if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::Arrow {
        // Parse as expression first (handles $var->method() and chained access)
        let expr = parse_expr(tokens, pos)?;
        // Check if followed by assignment: $var->prop = value;
        if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            // Extract property from the expression
            if let ExprKind::PropertyAccess { object, property } = expr.kind {
                return Ok(Stmt::new(StmtKind::PropertyAssign { object, property, value }, span));
            }
            return Err(CompileError::new(span, "Invalid assignment target"));
        }
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
    }

    // Array access: $var[...]
    if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::LBracket {
        *pos += 1; // consume $var
        *pos += 1; // consume [

        // $var[] = ... (push)
        if *pos < tokens.len() && tokens[*pos].0 == Token::RBracket {
            *pos += 1;
            expect_token(tokens, pos, &Token::Assign, "Expected '=' after '$var[]'")?;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            return Ok(Stmt::new(StmtKind::ArrayPush { array: name, value }, span));
        }

        // $var[index] = ...
        let index = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
            return Err(CompileError::new(span, "Expected ']'"));
        }
        *pos += 1;

        if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            return Ok(Stmt::new(StmtKind::ArrayAssign { array: name, index, value }, span));
        }

        return Err(CompileError::new(span, "Expected '=' after array access"));
    }

    // Post-increment/decrement
    if *pos + 1 < tokens.len() {
        match &tokens[*pos + 1].0 {
            Token::PlusPlus => {
                *pos += 2;
                expect_semicolon(tokens, pos)?;
                let expr = Expr::new(ExprKind::PostIncrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            Token::MinusMinus => {
                *pos += 2;
                expect_semicolon(tokens, pos)?;
                let expr = Expr::new(ExprKind::PostDecrement(name), span);
                return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
            }
            _ => {}
        }
    }

    // Closure call: $fn(args);
    if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::LParen {
        let expr = parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
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

fn parse_const_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'const'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected constant name after 'const'")),
    };
    *pos += 1;

    expect_token(tokens, pos, &Token::Assign, "Expected '=' after constant name")?;

    let value = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::ConstDecl { name, value }, span))
}

fn parse_list_unpack(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume '['

    let mut vars = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBracket {
        if !vars.is_empty() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(tokens[*pos].1, "Expected ',' between list variables"));
            }
            *pos += 1;
        }
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                vars.push(n.clone());
                *pos += 1;
            }
            _ => return Err(CompileError::new(span, "Expected variable in list unpacking")),
        }
    }

    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
        return Err(CompileError::new(span, "Expected ']' after list variables"));
    }
    *pos += 1;

    expect_token(tokens, pos, &Token::Assign, "Expected '=' after list pattern")?;

    let value = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::ListUnpack { vars, value }, span))
}

fn parse_global(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'global'

    let mut vars = Vec::new();
    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                vars.push(n.clone());
                *pos += 1;
            }
            _ => return Err(CompileError::new(span, "Expected variable after 'global'")),
        }
        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
        } else {
            break;
        }
    }

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Global { vars }, span))
}

fn parse_static_var(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'static'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected variable after 'static'")),
    };
    *pos += 1;

    expect_token(tokens, pos, &Token::Assign, "Expected '=' after static variable")?;

    let init = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::new(StmtKind::StaticVar { name, init }, span))
}

fn parse_function_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected function name")),
    };
    *pos += 1;

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after function name")?;

    let mut params = Vec::new();
    let mut variadic = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() || variadic.is_some() {
            expect_token(tokens, pos, &Token::Comma, "Expected ',' between parameters")?;
        }
        if variadic.is_some() {
            return Err(CompileError::new(span, "Variadic parameter must be the last parameter"));
        }
        // Check for & (pass by reference)
        let is_ref = if *pos < tokens.len() && tokens[*pos].0 == Token::Ampersand {
            *pos += 1;
            true
        } else {
            false
        };
        // Check for ... (variadic)
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            *pos += 1;
            match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Variable(n)) => {
                    variadic = Some(n.clone());
                    *pos += 1;
                }
                _ => return Err(CompileError::new(span, "Expected variable after '...'")),
            }
            continue;
        }
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                let n = n.clone();
                *pos += 1;
                // Check for default value
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                params.push((n, default, is_ref));
            }
            _ => return Err(CompileError::new(span, "Expected parameter variable")),
        }
    }
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after parameters")?;

    let body = parse_block(tokens, pos)?;

    Ok(Stmt::new(StmtKind::FunctionDecl { name, params, variadic, body }, span))
}

fn parse_return(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        return Ok(Stmt::new(StmtKind::Return(None), span));
    }

    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Return(Some(expr)), span))
}

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

/// Parse either a braced block `{ ... }` or a single statement (for braceless if/while/for/foreach).
pub fn parse_body(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Vec<Stmt>, CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        parse_block(tokens, pos)
    } else {
        let stmt = parse_stmt(tokens, pos)?;
        Ok(vec![stmt])
    }
}

fn expect_semicolon(tokens: &[(Token, Span)], pos: &mut usize) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        Ok(())
    } else {
        let span = if *pos < tokens.len() { tokens[*pos].1 } else { Span::dummy() };
        Err(CompileError::new(span, "Expected ';'"))
    }
}

/// Handle statements starting with $this: $this->prop = value; or $this->method();
fn parse_this_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    // Parse as expression first
    let expr = parse_expr(tokens, pos)?;
    // Check if followed by assignment
    if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
        *pos += 1;
        let value = parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        if let ExprKind::PropertyAccess { object, property } = expr.kind {
            return Ok(Stmt::new(StmtKind::PropertyAssign { object, property, value }, span));
        }
        return Err(CompileError::new(span, "Invalid assignment target after $this"));
    }
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
}

/// Parse a class declaration: class Name { properties and methods }
fn parse_class_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'class'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => { let n = n.clone(); *pos += 1; n }
        _ => return Err(CompileError::new(span, "Expected class name after 'class'")),
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after class name")?;

    let mut properties = Vec::new();
    let mut methods = Vec::new();

    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        let member_span = tokens[*pos].1;

        // Read optional visibility (default: public)
        let mut visibility = Visibility::Public;
        if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Public | Token::Private)) {
            visibility = match &tokens[*pos].0 {
                Token::Public => Visibility::Public,
                Token::Private => Visibility::Private,
                _ => unreachable!(),
            };
            *pos += 1;
        }

        // Read optional modifiers
        let mut is_static = false;
        let mut is_readonly = false;

        // Check for static and readonly in any order
        for _ in 0..2 {
            if *pos < tokens.len() && tokens[*pos].0 == Token::Static {
                is_static = true;
                *pos += 1;
            }
            if *pos < tokens.len() && tokens[*pos].0 == Token::ReadOnly {
                is_readonly = true;
                *pos += 1;
            }
        }

        if *pos >= tokens.len() {
            return Err(CompileError::new(member_span, "Unexpected end of class body"));
        }

        if tokens[*pos].0 == Token::Function {
            // Method declaration
            *pos += 1; // consume 'function'
            let method_name = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Identifier(n)) => { let n = n.clone(); *pos += 1; n }
                // __construct
                _ => return Err(CompileError::new(member_span, "Expected method name")),
            };

            // Parse parameters (same as function decl)
            expect_token(tokens, pos, &Token::LParen, "Expected '(' after method name")?;
            let mut params = Vec::new();
            let mut variadic = None;
            while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
                if !params.is_empty() {
                    expect_token(tokens, pos, &Token::Comma, "Expected ',' between parameters")?;
                }
                // Check for &$param (reference)
                let is_ref = *pos < tokens.len() && tokens[*pos].0 == Token::Ampersand;
                if is_ref { *pos += 1; }
                // Check for ...$param (variadic)
                if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
                    *pos += 1;
                    if let Some(Token::Variable(vn)) = tokens.get(*pos).map(|(t, _)| t) {
                        variadic = Some(vn.clone());
                        *pos += 1;
                    }
                    break;
                }
                let pname = match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Variable(n)) => { let n = n.clone(); *pos += 1; n }
                    _ => return Err(CompileError::new(member_span, "Expected parameter name")),
                };
                // Optional default value
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                params.push((pname, default, is_ref));
            }
            expect_token(tokens, pos, &Token::RParen, "Expected ')'")?;

            let body = parse_block(tokens, pos)?;

            methods.push(ClassMethod {
                name: method_name,
                visibility,
                is_static,
                params,
                variadic,
                body,
                span: member_span,
            });
        } else if let Some(Token::Variable(prop_name)) = tokens.get(*pos).map(|(t, _)| t.clone()) {
            // Property declaration
            let prop_name = prop_name.clone();
            *pos += 1;
            let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                *pos += 1;
                Some(parse_expr(tokens, pos)?)
            } else {
                None
            };
            expect_semicolon(tokens, pos)?;

            properties.push(ClassProperty {
                name: prop_name,
                visibility,
                readonly: is_readonly,
                default,
                span: member_span,
            });
        } else {
            return Err(CompileError::new(member_span, "Expected property or method declaration in class body"));
        }
    }

    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of class")?;

    Ok(Stmt::new(StmtKind::ClassDecl { name, properties, methods }, span))
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
        let span = if *pos < tokens.len() { tokens[*pos].1 } else { Span::dummy() };
        Err(CompileError::new(span, msg))
    }
}
