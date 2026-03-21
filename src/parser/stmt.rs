use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::Stmt;
use crate::parser::expr::parse_expr;

pub fn parse_stmt(tokens: &[Token], pos: &mut usize) -> Result<Stmt, CompileError> {
    match &tokens[*pos] {
        Token::Echo => parse_echo(tokens, pos),
        Token::Variable(_) => parse_assign(tokens, pos),
        other => Err(CompileError::at(
            0,
            0,
            &format!("Unexpected token at statement position: {:?}", other),
        )),
    }
}

fn parse_echo(tokens: &[Token], pos: &mut usize) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'echo'
    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::Echo(expr))
}

fn parse_assign(tokens: &[Token], pos: &mut usize) -> Result<Stmt, CompileError> {
    let name = match &tokens[*pos] {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };
    *pos += 1;

    if *pos >= tokens.len() || tokens[*pos] != Token::Assign {
        return Err(CompileError::at(0, 0, "Expected '=' after variable name"));
    }
    *pos += 1;

    let value = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(Stmt::Assign { name, value })
}

fn expect_semicolon(tokens: &[Token], pos: &mut usize) -> Result<(), CompileError> {
    if *pos < tokens.len() && tokens[*pos] == Token::Semicolon {
        *pos += 1;
        Ok(())
    } else {
        Err(CompileError::at(0, 0, "Expected ';'"))
    }
}
