use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::{Name, NameKind};
use crate::parser::ast::{
    CType, ClassMethod, ClassProperty, Expr, ExprKind, ExternField, ExternParam, PackedField,
    Stmt, StmtKind, TraitAdaptation, TraitUse, TypeExpr, UseItem, UseKind, Visibility,
};
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
        Token::Class => parse_class_decl(tokens, pos, span, false, false),
        Token::ReadOnly => parse_readonly_decl(tokens, pos, span),
        Token::Packed => parse_packed_decl(tokens, pos, span),
        Token::Interface => parse_interface_decl(tokens, pos, span),
        Token::Trait => parse_trait_decl(tokens, pos, span),
        Token::Abstract => parse_abstract_decl(tokens, pos, span),
        Token::Function => parse_function_decl(tokens, pos, span),
        Token::Namespace => parse_namespace_stmt(tokens, pos, span),
        Token::Use => parse_use_stmt(tokens, pos, span),
        Token::Return => parse_return(tokens, pos, span),
        Token::Throw => parse_throw(tokens, pos, span),
        Token::Include => parse_include(tokens, pos, span, false, false),
        Token::IncludeOnce => parse_include(tokens, pos, span, true, false),
        Token::Require => parse_include(tokens, pos, span, false, true),
        Token::RequireOnce => parse_include(tokens, pos, span, true, true),
        Token::Const => parse_const_decl(tokens, pos, span),
        Token::Global => parse_global(tokens, pos, span),
        Token::Static => {
            if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::DoubleColon {
                let expr = parse_expr(tokens, pos)?;
                expect_semicolon(tokens, pos)?;
                Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
            } else {
                parse_static_var(tokens, pos, span)
            }
        }
        Token::LBracket => parse_list_unpack(tokens, pos, span),
        Token::Identifier(_) | Token::Self_ | Token::Parent | Token::Backslash | Token::Question => {
            if looks_like_typed_assign(tokens, *pos) {
                return parse_typed_assign(tokens, pos, span);
            }
            let expr = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
        }
        // Control flow — delegated to control.rs
        Token::Switch => control::parse_switch(tokens, pos, span),
        Token::If => control::parse_if(tokens, pos, span),
        Token::IfDef => control::parse_ifdef(tokens, pos, span),
        Token::Try => control::parse_try(tokens, pos, span),
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

fn parse_namespace_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume namespace

    let name = if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        None
    } else {
        Some(parse_name(
            tokens,
            pos,
            span,
            "Expected namespace name after 'namespace'",
        )?)
    };

    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        return Ok(Stmt::new(StmtKind::NamespaceDecl { name }, span));
    }

    expect_token(tokens, pos, &Token::LBrace, "Expected ';' or '{' after namespace name")?;
    let mut body = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        body.push(parse_stmt(tokens, pos)?);
    }
    expect_token(tokens, pos, &Token::RBrace, "Expected '}' after namespace block")?;
    Ok(Stmt::new(StmtKind::NamespaceBlock { name, body }, span))
}

fn parse_use_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume use

    let default_kind = if *pos < tokens.len() && tokens[*pos].0 == Token::Function {
        *pos += 1;
        UseKind::Function
    } else if *pos < tokens.len() && tokens[*pos].0 == Token::Const {
        *pos += 1;
        UseKind::Const
    } else {
        UseKind::Class
    };

    let prefix = parse_use_prefix(tokens, pos, span)?;

    let imports = if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        parse_group_use_items(tokens, pos, span, prefix, default_kind.clone())?
    } else {
        vec![parse_single_use_item_after_name(
            tokens,
            pos,
            span,
            prefix,
            default_kind.clone(),
        )?]
    };

    let mut all_imports = imports;
    while *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
        *pos += 1;
        let item_kind = if *pos < tokens.len() && tokens[*pos].0 == Token::Function {
            *pos += 1;
            UseKind::Function
        } else if *pos < tokens.len() && tokens[*pos].0 == Token::Const {
            *pos += 1;
            UseKind::Const
        } else {
            default_kind.clone()
        };
        let name = parse_name(
            tokens,
            pos,
            span,
            "Expected imported name after ',' in use declaration",
        )?;
        all_imports.push(parse_single_use_item_after_name(tokens, pos, span, name, item_kind)?);
    }

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::UseDecl { imports: all_imports }, span))
}

/// Handle statements starting with $variable: assignment, array ops, or post-increment.
fn parse_variable_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let start_pos = *pos;
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

        if *pos < tokens.len() && tokens[*pos].0 == Token::Arrow {
            *pos = start_pos;
            let expr = parse_expr(tokens, pos)?;
            if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                *pos += 1;
                let value = parse_expr(tokens, pos)?;
                expect_semicolon(tokens, pos)?;
                if let ExprKind::PropertyAccess { object, property } = expr.kind {
                    return Ok(Stmt::new(StmtKind::PropertyAssign { object, property, value }, span));
                }
                return Err(CompileError::new(span, "Invalid assignment target"));
            }
            expect_semicolon(tokens, pos)?;
            return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
        }

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

fn looks_like_typed_assign(tokens: &[(Token, Span)], pos: usize) -> bool {
    let mut probe = pos;
    match parse_type_expr(tokens, &mut probe, tokens[pos].1) {
        Ok(_) => matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Variable(_))),
        Err(_) => false,
    }
}

fn parse_typed_assign(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let type_expr = parse_type_expr(tokens, pos, span)?;
    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(name)) => {
            let name = name.clone();
            *pos += 1;
            name
        }
        _ => return Err(CompileError::new(span, "Expected variable after type annotation")),
    };
    expect_token(tokens, pos, &Token::Assign, "Expected '=' after typed variable")?;
    let value = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        },
        span,
    ))
}

pub(crate) fn parse_type_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    let mut ty = if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Question)) {
        *pos += 1;
        TypeExpr::Nullable(Box::new(parse_atomic_type_expr(tokens, pos, span)?))
    } else {
        parse_atomic_type_expr(tokens, pos, span)?
    };

    if matches!(ty, TypeExpr::Nullable(_))
        && matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Pipe))
    {
        return Err(CompileError::new(
            span,
            "Nullable shorthand cannot be combined directly with union types; write T|null",
        ));
    }

    let mut members = vec![ty];
    while matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Pipe)) {
        *pos += 1;
        members.push(parse_atomic_type_expr(tokens, pos, span)?);
    }

    if members.len() == 1 {
        ty = members.pop().expect("type member exists");
        Ok(ty)
    } else {
        Ok(TypeExpr::Union(members))
    }
}

fn parse_atomic_type_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(name)) if matches!(name.as_str(), "int" | "integer") => {
            *pos += 1;
            Ok(TypeExpr::Int)
        }
        Some(Token::Identifier(name)) if matches!(name.as_str(), "float" | "double" | "real") => {
            *pos += 1;
            Ok(TypeExpr::Float)
        }
        Some(Token::Identifier(name)) if matches!(name.as_str(), "bool" | "boolean") => {
            *pos += 1;
            Ok(TypeExpr::Bool)
        }
        Some(Token::Identifier(name)) if matches!(name.as_str(), "ptr" | "pointer") => {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == Token::Less {
                *pos += 1;
                let target = parse_name(
                    tokens,
                    pos,
                    span,
                    "Expected pointer target type inside ptr<...>",
                )?;
                expect_token(tokens, pos, &Token::Greater, "Expected '>' after ptr target type")?;
                Ok(TypeExpr::Ptr(Some(target)))
            } else {
                Ok(TypeExpr::Ptr(None))
            }
        }
        Some(Token::Identifier(name)) if name == "buffer" => {
            *pos += 1;
            expect_token(tokens, pos, &Token::Less, "Expected '<' after buffer")?;
            let inner = parse_type_expr(tokens, pos, span)?;
            expect_token(tokens, pos, &Token::Greater, "Expected '>' after buffer element type")?;
            Ok(TypeExpr::Buffer(Box::new(inner)))
        }
        Some(Token::Identifier(_)) | Some(Token::Backslash) => Ok(TypeExpr::Named(parse_name(
            tokens,
            pos,
            span,
            "Expected type name",
        )?)),
        _ => Err(CompileError::new(span, "Expected type expression")),
    }
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

fn parse_throw(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Throw(expr), span))
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

pub(crate) fn expect_semicolon(tokens: &[(Token, Span)], pos: &mut usize) -> Result<(), CompileError> {
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

/// Parse a class declaration: class Name { use TraitName; properties and methods }
fn parse_class_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    is_abstract: bool,
    is_readonly_class: bool,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'class'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => { let n = n.clone(); *pos += 1; n }
        _ => return Err(CompileError::new(span, "Expected class name after 'class'")),
    };

    let extends = if *pos < tokens.len() && tokens[*pos].0 == Token::Extends {
        *pos += 1;
        Some(parse_name(
            tokens,
            pos,
            span,
            "Expected parent class name after 'extends'",
        )?)
    } else {
        None
    };

    let implements = if *pos < tokens.len() && tokens[*pos].0 == Token::Implements {
        *pos += 1;
        parse_name_list(tokens, pos, span, "Expected interface name after 'implements'")?
    } else {
        Vec::new()
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after class name")?;

    let (trait_uses, properties, methods) = parse_class_like_body(tokens, pos, "class")?;

    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of class")?;

    Ok(Stmt::new(
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        },
        span,
    ))
}

fn parse_packed_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'packed'
    expect_token(tokens, pos, &Token::Class, "Expected 'class' after 'packed'")?;

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected class name after 'packed class'")),
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after packed class name")?;
    let mut fields = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        let field_span = tokens[*pos].1;
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Public) => *pos += 1,
            Some(Token::Protected | Token::Private | Token::Static | Token::ReadOnly | Token::Abstract) => {
                return Err(CompileError::new(
                    field_span,
                    "Packed class fields may only use public visibility",
                ))
            }
            _ => {}
        }

        let type_expr = parse_type_expr(tokens, pos, field_span)?;
        let field_name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(name)) => {
                let name = name.clone();
                *pos += 1;
                name
            }
            _ => {
                return Err(CompileError::new(
                    field_span,
                    "Expected $field after packed field type",
                ))
            }
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            return Err(CompileError::new(
                field_span,
                "Packed class fields cannot have default values",
            ));
        }
        expect_semicolon(tokens, pos)?;
        fields.push(PackedField {
            name: field_name,
            type_expr,
            span: field_span,
        });
    }

    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of packed class")?;
    Ok(Stmt::new(StmtKind::PackedClassDecl { name, fields }, span))
}

fn parse_abstract_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'abstract'
    if *pos < tokens.len() && tokens[*pos].0 == Token::ReadOnly {
        *pos += 1; // consume 'readonly'
        if *pos < tokens.len() && tokens[*pos].0 == Token::Class {
            return parse_class_decl(tokens, pos, span, true, true);
        }
        return Err(CompileError::new(
            span,
            "Expected 'class' after 'abstract readonly' at statement position",
        ));
    }
    if *pos < tokens.len() && tokens[*pos].0 == Token::Class {
        return parse_class_decl(tokens, pos, span, true, false);
    }
    Err(CompileError::new(
        span,
        "Expected 'class' after 'abstract' at statement position",
    ))
}

fn parse_readonly_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'readonly'
    if *pos < tokens.len() && tokens[*pos].0 == Token::Abstract {
        *pos += 1; // consume 'abstract'
        if *pos < tokens.len() && tokens[*pos].0 == Token::Class {
            return parse_class_decl(tokens, pos, span, true, true);
        }
        return Err(CompileError::new(
            span,
            "Expected 'class' after 'readonly abstract' at statement position",
        ));
    }
    if *pos < tokens.len() && tokens[*pos].0 == Token::Class {
        return parse_class_decl(tokens, pos, span, false, true);
    }
    Err(CompileError::new(
        span,
        "Expected 'class' after 'readonly' at statement position",
    ))
}

fn parse_interface_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'interface'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected interface name after 'interface'")),
    };

    let extends = if *pos < tokens.len() && tokens[*pos].0 == Token::Extends {
        *pos += 1;
        parse_name_list(tokens, pos, span, "Expected parent interface name after 'extends'")?
    } else {
        Vec::new()
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after interface name")?;
    let methods = parse_interface_body(tokens, pos)?;
    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of interface")?;

    Ok(Stmt::new(
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        },
        span,
    ))
}

/// Parse a trait declaration: trait Name { use OtherTrait; properties and methods }
fn parse_trait_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'trait'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected trait name after 'trait'")),
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after trait name")?;
    let (trait_uses, properties, methods) = parse_class_like_body(tokens, pos, "trait")?;
    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of trait")?;

    Ok(Stmt::new(
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        },
        span,
    ))
}

fn parse_class_like_body(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    owner_kind: &str,
) -> Result<(Vec<TraitUse>, Vec<ClassProperty>, Vec<ClassMethod>), CompileError> {
    let mut trait_uses = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();

    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        let member_span = tokens[*pos].1;
        if tokens[*pos].0 == Token::Use {
            trait_uses.push(parse_trait_use(tokens, pos, member_span)?);
            continue;
        }

        let modifiers = parse_member_modifiers(tokens, pos);

        if *pos >= tokens.len() {
            return Err(CompileError::new(
                member_span,
                &format!("Unexpected end of {} body", owner_kind),
            ));
        }

        if tokens[*pos].0 == Token::Function {
            if modifiers.is_readonly {
                return Err(CompileError::new(
                    member_span,
                    "Readonly methods are not supported",
                ));
            }
            methods.push(parse_class_like_method(
                tokens,
                pos,
                member_span,
                modifiers.visibility,
                modifiers.is_static,
                modifiers.is_abstract,
            )?);
            continue;
        }

        if let Some(Token::Variable(prop_name)) = tokens.get(*pos).map(|(t, _)| t.clone()) {
            if modifiers.is_static {
                return Err(CompileError::new(
                    member_span,
                    "Static properties are not supported",
                ));
            }
            if modifiers.is_abstract {
                return Err(CompileError::new(
                    member_span,
                    "Abstract properties are not supported",
                ));
            }
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
                visibility: modifiers.visibility,
                readonly: modifiers.is_readonly,
                default,
                span: member_span,
            });
            continue;
        }

        return Err(CompileError::new(
            member_span,
            &format!(
                "Expected trait use, property, or method declaration in {} body",
                owner_kind
            ),
        ));
    }

    Ok((trait_uses, properties, methods))
}

struct MemberModifiers {
    visibility: Visibility,
    is_static: bool,
    is_readonly: bool,
    is_abstract: bool,
}

fn parse_member_modifiers(tokens: &[(Token, Span)], pos: &mut usize) -> MemberModifiers {
    let mut visibility = Visibility::Public;
    let mut is_static = false;
    let mut is_readonly = false;
    let mut is_abstract = false;

    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Public) => {
                visibility = Visibility::Public;
                *pos += 1;
            }
            Some(Token::Protected) => {
                visibility = Visibility::Protected;
                *pos += 1;
            }
            Some(Token::Private) => {
                visibility = Visibility::Private;
                *pos += 1;
            }
            Some(Token::Static) => {
                is_static = true;
                *pos += 1;
            }
            Some(Token::ReadOnly) => {
                is_readonly = true;
                *pos += 1;
            }
            Some(Token::Abstract) => {
                is_abstract = true;
                *pos += 1;
            }
            _ => break,
        }
    }

    MemberModifiers {
        visibility,
        is_static,
        is_readonly,
        is_abstract,
    }
}

fn parse_class_like_method(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    visibility: Visibility,
    is_static: bool,
    is_abstract: bool,
) -> Result<ClassMethod, CompileError> {
    *pos += 1; // consume 'function'
    let method_name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected method name")),
    };

    expect_token(tokens, pos, &Token::LParen, "Expected '(' after method name")?;
    let (params, variadic) = parse_params(tokens, pos, span)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')'")?;
    let (has_body, body) = if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        (false, Vec::new())
    } else {
        (true, parse_block(tokens, pos)?)
    };
    Ok(ClassMethod {
        name: method_name,
        visibility,
        is_static,
        is_abstract,
        has_body,
        params,
        variadic,
        body,
        span,
    })
}

fn parse_interface_body(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Vec<ClassMethod>, CompileError> {
    let mut methods = Vec::new();

    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        let member_span = tokens[*pos].1;
        let modifiers = parse_member_modifiers(tokens, pos);
        if *pos >= tokens.len() || tokens[*pos].0 != Token::Function {
            return Err(CompileError::new(
                member_span,
                "Interfaces may only contain method declarations",
            ));
        }
        methods.push(parse_class_like_method(
            tokens,
            pos,
            member_span,
            modifiers.visibility,
            modifiers.is_static,
            true,
        )?);
    }

    Ok(methods)
}

pub(crate) fn name_starts_at(tokens: &[(Token, Span)], pos: usize) -> bool {
    matches!(
        tokens.get(pos).map(|(t, _)| t),
        Some(Token::Identifier(_)) | Some(Token::Backslash)
    )
}

pub(crate) fn parse_name(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    first_error: &str,
) -> Result<Name, CompileError> {
    let mut kind = NameKind::Unqualified;
    if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
        kind = NameKind::FullyQualified;
        *pos += 1;
    }

    let mut parts = Vec::new();
    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => {
                parts.push(name.clone());
                *pos += 1;
            }
            _ if parts.is_empty() => return Err(CompileError::new(span, first_error)),
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected identifier after '\\' in qualified name",
                ))
            }
        }

        if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
            if kind != NameKind::FullyQualified {
                kind = NameKind::Qualified;
            }
            *pos += 1;
            continue;
        }
        break;
    }

    Ok(Name::from_parts(kind, parts))
}

fn parse_optional_alias(tokens: &[(Token, Span)], pos: &mut usize) -> Option<String> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::As {
        *pos += 1;
        if let Some(Token::Identifier(alias)) = tokens.get(*pos).map(|(t, _)| t) {
            let alias = alias.clone();
            *pos += 1;
            return Some(alias);
        }
    }
    None
}

fn parse_single_use_item_after_name(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    name: Name,
    kind: UseKind,
) -> Result<UseItem, CompileError> {
    let alias = parse_optional_alias(tokens, pos)
        .or_else(|| name.last_segment().map(str::to_string))
        .ok_or_else(|| CompileError::new(span, "Imported name cannot be empty"))?;
    Ok(UseItem { kind, name, alias })
}

fn parse_group_use_items(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    prefix: Name,
    default_kind: UseKind,
) -> Result<Vec<UseItem>, CompileError> {
    expect_token(tokens, pos, &Token::LBrace, "Expected '{' in group use declaration")?;
    let mut imports = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        if !imports.is_empty() {
            expect_token(tokens, pos, &Token::Comma, "Expected ',' in group use declaration")?;
        }

        let kind = if *pos < tokens.len() && tokens[*pos].0 == Token::Function {
            *pos += 1;
            UseKind::Function
        } else if *pos < tokens.len() && tokens[*pos].0 == Token::Const {
            *pos += 1;
            UseKind::Const
        } else {
            default_kind.clone()
        };

        let suffix = parse_name(
            tokens,
            pos,
            span,
            "Expected imported name inside group use declaration",
        )?;
        if suffix.is_fully_qualified() {
            return Err(CompileError::new(
                span,
                "Group use items must be relative to the shared prefix",
            ));
        }
        let mut parts = prefix.parts.clone();
        parts.extend(suffix.parts);
        let name = Name::qualified(parts);
        imports.push(parse_single_use_item_after_name(tokens, pos, span, name, kind)?);
    }
    expect_token(tokens, pos, &Token::RBrace, "Expected '}' after group use declaration")?;
    Ok(imports)
}

fn parse_use_prefix(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Name, CompileError> {
    let mut kind = NameKind::Unqualified;
    if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
        kind = NameKind::FullyQualified;
        *pos += 1;
    }

    let mut parts = Vec::new();
    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => {
                parts.push(name.clone());
                *pos += 1;
            }
            _ if parts.is_empty() => {
                return Err(CompileError::new(span, "Expected imported name after 'use'"))
            }
            _ => break,
        }

        if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
            if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::LBrace {
                *pos += 1;
                break;
            }
            if kind != NameKind::FullyQualified {
                kind = NameKind::Qualified;
            }
            *pos += 1;
            continue;
        }
        break;
    }

    Ok(Name::from_parts(kind, parts))
}

fn parse_name_list(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    first_error: &str,
) -> Result<Vec<Name>, CompileError> {
    let mut names = Vec::new();
    loop {
        if !name_starts_at(tokens, *pos) {
            if names.is_empty() {
                return Err(CompileError::new(span, first_error));
            }
            return Err(CompileError::new(
                span,
                "Expected name after ',' in declaration list",
            ));
        }
        names.push(parse_name(tokens, pos, span, first_error)?);

        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
            continue;
        }
        break;
    }
    Ok(names)
}

fn parse_params(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<(Vec<(String, Option<Expr>, bool)>, Option<String>), CompileError> {
    let mut params = Vec::new();
    let mut variadic = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() || variadic.is_some() {
            expect_token(tokens, pos, &Token::Comma, "Expected ',' between parameters")?;
        }
        if variadic.is_some() {
            return Err(CompileError::new(
                span,
                "Variadic parameter must be the last parameter",
            ));
        }
        let is_ref = if *pos < tokens.len() && tokens[*pos].0 == Token::Ampersand {
            *pos += 1;
            true
        } else {
            false
        };
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
    Ok((params, variadic))
}

fn parse_trait_use(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<TraitUse, CompileError> {
    *pos += 1; // consume 'use'
    let mut trait_names = Vec::new();
    loop {
        trait_names.push(parse_name(tokens, pos, span, "Expected trait name after 'use'")?);
        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
            continue;
        }
        break;
    }

    let mut adaptations = Vec::new();
    if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        *pos += 1;
        while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
            let (trait_name, method) = parse_trait_adaptation_target(tokens, pos, span)?;
            if *pos >= tokens.len() {
                return Err(CompileError::new(span, "Unexpected end of trait adaptation block"));
            }
            match &tokens[*pos].0 {
                Token::As => {
                    *pos += 1;
                    let visibility = match tokens.get(*pos).map(|(t, _)| t) {
                        Some(Token::Public) => {
                            *pos += 1;
                            Some(Visibility::Public)
                        }
                        Some(Token::Protected) => {
                            *pos += 1;
                            Some(Visibility::Protected)
                        }
                        Some(Token::Private) => {
                            *pos += 1;
                            Some(Visibility::Private)
                        }
                        _ => None,
                    };
                    let alias = match tokens.get(*pos).map(|(t, _)| t) {
                        Some(Token::Identifier(name)) => {
                            let name = name.clone();
                            *pos += 1;
                            Some(name)
                        }
                        _ => None,
                    };
                    if visibility.is_none() && alias.is_none() {
                        return Err(CompileError::new(
                            span,
                            "Trait alias adaptation requires a visibility and/or alias name",
                        ));
                    }
                    adaptations.push(TraitAdaptation::Alias {
                        trait_name,
                        method,
                        alias,
                        visibility,
                    });
                }
                Token::InsteadOf => {
                    *pos += 1;
                    let mut instead_of = Vec::new();
                    loop {
                        match tokens.get(*pos).map(|(t, _)| t) {
                            Some(Token::Identifier(_)) | Some(Token::Backslash) => {
                                instead_of.push(parse_name(
                                    tokens,
                                    pos,
                                    span,
                                    "Expected trait name after 'insteadof'",
                                )?);
                            }
                            _ => {
                                return Err(CompileError::new(
                                    span,
                                    "Expected trait name after 'insteadof'",
                                ))
                            }
                        }
                        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                            *pos += 1;
                            continue;
                        }
                        break;
                    }
                    if instead_of.is_empty() {
                        return Err(CompileError::new(
                            span,
                            "Trait insteadof adaptation requires at least one suppressed trait",
                        ));
                    }
                    adaptations.push(TraitAdaptation::InsteadOf {
                        trait_name,
                        method,
                        instead_of,
                    });
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "Expected 'as' or 'insteadof' inside trait adaptation block",
                    ))
                }
            }
            expect_semicolon(tokens, pos)?;
        }
        expect_token(tokens, pos, &Token::RBrace, "Expected '}' after trait adaptations")?;
        if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
            *pos += 1;
        }
    } else {
        expect_semicolon(tokens, pos)?;
    }
    Ok(TraitUse {
        trait_names,
        adaptations,
        span,
    })
}

fn parse_trait_adaptation_target(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<(Option<Name>, String), CompileError> {
    let first = parse_name(tokens, pos, span, "Expected method or trait name in adaptation")?;
    if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleColon {
        *pos += 1;
        let method = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => {
                let name = name.clone();
                *pos += 1;
                name
            }
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected method name after 'TraitName::' in adaptation",
                ))
            }
        };
        Ok((Some(first), method))
    } else {
        let method = first
            .last_segment()
            .map(str::to_string)
            .ok_or_else(|| CompileError::new(span, "Expected method name in adaptation"))?;
        Ok((None, method))
    }
}

pub(crate) fn expect_token(
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

// --- FFI parsing ---

fn parse_c_type(tokens: &[(Token, Span)], pos: &mut usize) -> Result<CType, CompileError> {
    let span = if *pos < tokens.len() { tokens[*pos].1 } else { Span::dummy() };
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
                            Some(Token::Identifier(t)) => { let t = t.clone(); *pos += 1; t }
                            _ => return Err(CompileError::new(span, "Expected type name after 'ptr<'")),
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
                _ => Err(CompileError::new(span, &format!("Unknown C type: {}", name))),
            }
        }
        _ => Err(CompileError::new(span, "Expected type name")),
    }
}

fn parse_extern_params(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Vec<ExternParam>, CompileError> {
    let mut params = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(tokens[*pos].1, "Expected ',' between extern parameters"));
            }
            *pos += 1;
        }
        let c_type = parse_c_type(tokens, pos)?;
        let name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => { let n = n.clone(); *pos += 1; n }
            _ => return Err(CompileError::new(
                if *pos < tokens.len() { tokens[*pos].1 } else { Span::dummy() },
                "Expected $parameter_name after type",
            )),
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
        Some(Token::Identifier(n)) => { let n = n.clone(); *pos += 1; n }
        _ => return Err(CompileError::new(span, "Expected function name after 'extern function'")),
    };
    expect_token(tokens, pos, &Token::LParen, "Expected '(' after extern function name")?;
    let params = parse_extern_params(tokens, pos)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after extern parameters")?;

    // Parse return type: ': type'
    let return_type = if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        parse_c_type(tokens, pos)?
    } else {
        CType::Void
    };

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::ExternFunctionDecl { name, params, return_type, library }, span))
}

/// Parse extern declarations. Returns Vec<Stmt> because extern "lib" { } blocks produce multiple stmts.
/// Called from parse() in mod.rs, not from parse_stmt.
pub fn parse_extern_stmts(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Vec<Stmt>, CompileError> {
    let span = tokens[*pos].1;
    *pos += 1; // consume 'extern'

    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Function) => {
            Ok(vec![parse_extern_function(tokens, pos, span, None)?])
        }

        Some(Token::StringLiteral(lib)) => {
            let library = lib.clone();
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == Token::Function {
                // extern "lib" function name(): type;
                return Ok(vec![parse_extern_function(tokens, pos, span, Some(library))?]);
            }
            // extern "lib" { function ...; function ...; }
            expect_token(tokens, pos, &Token::LBrace, "Expected '{' or 'function' after extern library name")?;
            let mut stmts = Vec::new();
            while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
                if tokens[*pos].0 != Token::Function {
                    return Err(CompileError::new(tokens[*pos].1, "Expected 'function' inside extern block"));
                }
                stmts.push(parse_extern_function(tokens, pos, span, Some(library.clone()))?);
            }
            expect_token(tokens, pos, &Token::RBrace, "Expected '}' after extern block")?;
            if stmts.is_empty() {
                return Err(CompileError::new(span, "Empty extern block"));
            }
            Ok(stmts)
        }

        Some(Token::Class) => {
            *pos += 1; // consume 'class'
            let name = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Identifier(n)) => { let n = n.clone(); *pos += 1; n }
                _ => return Err(CompileError::new(span, "Expected class name after 'extern class'")),
            };
            expect_token(tokens, pos, &Token::LBrace, "Expected '{' after extern class name")?;
            let mut fields = Vec::new();
            while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
                if tokens[*pos].0 == Token::Public {
                    *pos += 1;
                }
                let c_type = parse_c_type(tokens, pos)?;
                let field_name = match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Variable(n)) => { let n = n.clone(); *pos += 1; n }
                    _ => return Err(CompileError::new(
                        if *pos < tokens.len() { tokens[*pos].1 } else { Span::dummy() },
                        "Expected $field_name in extern class",
                    )),
                };
                expect_semicolon(tokens, pos)?;
                fields.push(ExternField { name: field_name, c_type });
            }
            expect_token(tokens, pos, &Token::RBrace, "Expected '}' after extern class body")?;
            Ok(vec![Stmt::new(StmtKind::ExternClassDecl { name, fields }, span)])
        }

        Some(Token::Global) => {
            *pos += 1; // consume 'global'
            let c_type = parse_c_type(tokens, pos)?;
            let name = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Variable(n)) => { let n = n.clone(); *pos += 1; n }
                _ => return Err(CompileError::new(span, "Expected $variable_name after extern global type")),
            };
            expect_semicolon(tokens, pos)?;
            Ok(vec![Stmt::new(StmtKind::ExternGlobalDecl { name, c_type }, span)])
        }

        _ => Err(CompileError::new(span, "Expected 'function', string literal, 'class', or 'global' after 'extern'")),
    }
}
