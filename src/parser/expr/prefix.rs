//! Purpose:
//! Parses prefix and primary PHP expressions before Pratt suffix handling takes over.
//! Covers literals, variables, names, arrays, magic constants, unary operators, and grouped expressions.
//!
//! Called from:
//! - `crate::parser::expr::pratt::parse_expr_bp()`.
//!
//! Key details:
//! - `__LINE__` is lowered at parse time while other magic constants remain AST nodes for later context passes.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::names::Name;
use crate::parser::ast::{ArrayEntry, Expr, ExprKind, MagicConstant, StaticReceiver};
use crate::span::Span;

use super::calls::{parse_scoped_static_call, peek_cast};
use super::prefix_complex::{
    parse_arrow_closure, parse_attributed_closure, parse_closure, parse_function_call_or_callable,
    parse_match_expr, parse_named_expr, parse_new_object,
};
use super::pratt::parse_expr_bp;
use super::{parse_args, parse_expr};

/// Parses a prefix (unary or primary) PHP expression and returns the resulting AST node.
/// Dispatched from `parse_expr_bp` for the leftmost expression in a binding-power loop.
/// Advances `pos` past all tokens consumed by the prefix; the caller continues with the
/// remaining token stream. Returns an error on unexpected end of input or unrecognized tokens.
pub(super) fn parse_prefix(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<Expr, CompileError> {
    if *pos >= tokens.len() {
        let span = tokens
            .last()
            .map(|(_, metadata)| metadata.span)
            .unwrap_or(Span::dummy());
        return Err(CompileError::new(span, "Unexpected end of input"));
    }

    let span = tokens[*pos].1.span;

    // Predefined tokens remain name segments before namespace or class access.
    if crate::parser::stmt::token_as_import_name(&tokens[*pos].0, &tokens[*pos].1).is_some()
        && matches!(tokens.get(*pos + 1), Some((Token::Backslash | Token::DoubleColon, _)))
    {
        return parse_named_expr(tokens, pos, span);
    }

    // `\PHP_EOL`, `\PHP_INT_MAX`, `\M_PI`, `\true`...: the lexer turns these predefined
    // constants into dedicated tokens, so the name parser never sees an identifier after the `\`
    // (#1307). The fully qualified spelling names the same global constant, so it parses as the
    // bare token. The few that stay constant references keep their fully qualified kind, so a
    // namespace-local constant of the same name can never shadow them.
    // `\static::class` / `\self::class` / `\parent::class` resolve like the unqualified fetch
    // in PHP 8; any other fully qualified use of those words is rejected by `parse_name`.
    if tokens[*pos].0 == Token::Backslash
        && matches!(
            tokens.get(*pos + 1),
            Some((Token::Self_ | Token::Parent | Token::Static, _))
        )
        && matches!(tokens.get(*pos + 2), Some((Token::DoubleColon, _)))
        && matches!(tokens.get(*pos + 3), Some((Token::Class, _)))
    {
        *pos += 1;
        let mut expr = parse_prefix(tokens, pos)?;
        expr.span = Span::with_end_from(span, expr.span);
        return Ok(expr);
    }

    if tokens[*pos].0 == Token::Backslash {
        if let Some((next, metadata)) = tokens.get(*pos + 1) {
            let literal_constant = matches!(next, Token::True | Token::False | Token::Null);
            let continues_name = matches!(
                tokens.get(*pos + 2),
                Some((Token::Backslash | Token::DoubleColon | Token::LParen, _))
            );
            if literal_constant
                || (crate::parser::stmt::token_as_import_name(next, metadata).is_some()
                    && !continues_name)
            {
                *pos += 1;
                let mut expr = parse_prefix(tokens, pos)?;
                if let ExprKind::ConstRef(name) = &expr.kind {
                    if name.kind == crate::names::NameKind::Unqualified {
                        expr.kind = ExprKind::ConstRef(Name::from_parts(
                            crate::names::NameKind::FullyQualified,
                            name.parts.clone(),
                        ));
                    }
                }
                // The expression starts at the `\`, so diagnostics point at the whole name.
                expr.span = Span::with_end_from(span, expr.span);
                return Ok(expr);
            }
        }
    }

    match &tokens[*pos].0 {
        Token::Minus => parse_unary(tokens, pos, span, ExprKind::Negate, 35),
        Token::Plus => parse_unary_plus(tokens, pos, span),
        Token::Include | Token::IncludeOnce | Token::Require | Token::RequireOnce => {
            parse_include_value(tokens, pos, span)
        }
        Token::Bang => parse_unary(tokens, pos, span, ExprKind::Not, 35),
        Token::Tilde => parse_unary(tokens, pos, span, ExprKind::BitNot, 35),
        Token::At => parse_unary(tokens, pos, span, ExprKind::ErrorSuppress, 35),
        Token::Print => parse_unary(tokens, pos, span, ExprKind::Print, 7),
        Token::Throw => parse_unary(tokens, pos, span, ExprKind::Throw, 0),
        Token::Clone => parse_clone(tokens, pos, span),
        Token::True => parse_simple(tokens, pos, span, ExprKind::BoolLiteral(true)),
        Token::False => parse_simple(tokens, pos, span, ExprKind::BoolLiteral(false)),
        Token::Null => parse_simple(tokens, pos, span, ExprKind::Null),
        Token::Inf => parse_simple(tokens, pos, span, ExprKind::FloatLiteral(f64::INFINITY)),
        Token::Nan => parse_simple(tokens, pos, span, ExprKind::FloatLiteral(f64::NAN)),
        Token::PhpIntMax => parse_simple(tokens, pos, span, ExprKind::IntLiteral(i64::MAX)),
        Token::PhpIntMin => parse_simple(tokens, pos, span, ExprKind::IntLiteral(i64::MIN)),
        Token::PhpFloatMax => parse_simple(tokens, pos, span, ExprKind::FloatLiteral(f64::MAX)),
        Token::MPi => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::PI),
        ),
        Token::ME => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::E),
        ),
        Token::MSqrt2 => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::SQRT_2),
        ),
        Token::MPi2 => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::FRAC_PI_2),
        ),
        Token::MPi4 => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::FRAC_PI_4),
        ),
        Token::MLog2e => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::LOG2_E),
        ),
        Token::MLog10e => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::LOG10_E),
        ),
        Token::PhpFloatMin => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(f64::MIN_POSITIVE),
        ),
        Token::PhpFloatEpsilon => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(f64::EPSILON),
        ),
        Token::Stdin => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::ConstRef(Name::unqualified("STDIN")),
        ),
        Token::Stdout => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::ConstRef(Name::unqualified("STDOUT")),
        ),
        Token::Stderr => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::ConstRef(Name::unqualified("STDERR")),
        ),
        Token::PhpEol => parse_simple(tokens, pos, span, ExprKind::StringLiteral("\n".to_string())),
        Token::PhpOs => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::ConstRef(Name::unqualified("PHP_OS")),
        ),
        Token::DirectorySeparator => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::StringLiteral("/".to_string()),
        ),
        Token::DunderLine => {
            parse_simple(tokens, pos, span, ExprKind::IntLiteral(span.line as i64))
        }
        Token::DunderDir => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Dir),
        ),
        Token::DunderFile => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::File),
        ),
        Token::DunderFunction => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Function),
        ),
        Token::DunderClass => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Class),
        ),
        Token::DunderMethod => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Method),
        ),
        Token::DunderNamespace => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Namespace),
        ),
        Token::DunderTrait => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Trait),
        ),
        Token::PlusPlus => parse_prefix_inc_dec(tokens, pos, span, true),
        Token::MinusMinus => parse_prefix_inc_dec(tokens, pos, span, false),
        Token::StringLiteral(value) => {
            let value = value.clone();
            *pos += 1;
            Ok(Expr::new(ExprKind::StringLiteral(value), span))
        }
        Token::IntLiteral(value) => {
            let value = *value;
            *pos += 1;
            Ok(Expr::new(ExprKind::IntLiteral(value), span))
        }
        Token::FloatLiteral(value) => {
            let value = *value;
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(value), span))
        }
        Token::Variable(name) => parse_variable(tokens, pos, span, name.clone()),
        Token::Dollar => crate::parser::variable_variables::parse_variable_variable(tokens, pos, span),
        Token::LParen => parse_group_or_cast(tokens, pos, span),
        Token::LBracket => parse_array_literal(tokens, pos, span),
        Token::Match => parse_match_expr(tokens, pos, span),
        Token::Function => parse_closure(tokens, pos, span, false),
        Token::Fn => parse_arrow_closure(tokens, pos, span, false),
        Token::AttrOpen => parse_attributed_closure(tokens, pos, span),
        Token::Identifier(_) | Token::Enum | Token::Backslash => {
            parse_named_expr(tokens, pos, span)
        }
        Token::Self_ => {
            *pos += 1;
            parse_scoped_static_call(tokens, pos, span, StaticReceiver::Self_, "self")
        }
        Token::Static => {
            // `static function() {}` and `static fn() => ...` — closures that
            // do not capture $this. Routed here before parse_scoped_static_call.
            match tokens.get(*pos + 1).map(|(t, _)| t) {
                Some(Token::Function) => {
                    *pos += 1; // consume `static`, leave `function` for parse_closure
                    parse_closure(tokens, pos, span, true)
                }
                Some(Token::Fn) => {
                    *pos += 1; // consume `static`, leave `fn` for parse_arrow_closure
                    parse_arrow_closure(tokens, pos, span, true)
                }
                _ => {
                    *pos += 1;
                    parse_scoped_static_call(tokens, pos, span, StaticReceiver::Static, "static")
                }
            }
        }
        Token::Parent => {
            *pos += 1;
            parse_scoped_static_call(tokens, pos, span, StaticReceiver::Parent, "parent")
        }
        Token::New => parse_new_object(tokens, pos, span),
        // `$this(...)` invokes the current object exactly like `($this)(...)`: through
        // `__invoke` only. A plain `__invoke` method call would fall back to `__call`, which PHP
        // never does for an object invocation.
        Token::This if matches!(tokens.get(*pos + 1), Some((Token::LParen, _))) => {
            let this = Expr::new(ExprKind::This, span);
            *pos += 2;
            if super::calls::parse_first_class_callable_parens(tokens, pos)? {
                // `$this(...)`: the Closure of the invokable current object.
                let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
                return Ok(expr_first_class_callable(this, span));
            }
            let args = parse_args(tokens, pos, span)?;
            let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
            Ok(Expr::new(
                ExprKind::ExprCall {
                    callee: Box::new(this),
                    args,
                },
                span,
            ))
        }
        Token::This => parse_simple(tokens, pos, span, ExprKind::This),
        Token::Yield => parse_yield(tokens, pos, span),
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token: {:?}", other),
        )),
    }
}

/// Parses `yield` and `yield from` expressions. Consumes the `yield` token and optionally
/// parses a following expression or key => value pair. Returns a `Yield` or `YieldFrom` node
/// using the given span. On end of input or a terminating token, returns bare `Yield { key: None, value: None }`.
fn parse_yield(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;

    if *pos >= tokens.len() {
        return Ok(Expr::new(
            ExprKind::Yield {
                key: None,
                value: None,
            },
            span,
        ));
    }

    if let Token::Identifier(name) = &tokens[*pos].0 {
        if name.eq_ignore_ascii_case("from") {
            *pos += 1;
            let inner = parse_expr_bp(tokens, pos, 0)?;
            return Ok(Expr::new(ExprKind::YieldFrom(Box::new(inner)), span));
        }
    }

    match &tokens[*pos].0 {
        Token::Semicolon
        | Token::RParen
        | Token::RBracket
        | Token::RBrace
        | Token::Comma
        | Token::Eof => {
            return Ok(Expr::new(
                ExprKind::Yield {
                    key: None,
                    value: None,
                },
                span,
            ));
        }
        _ => {}
    }

    let first = parse_expr_bp(tokens, pos, 0)?;
    if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
        *pos += 1;
        let value = parse_expr_bp(tokens, pos, 0)?;
        return Ok(Expr::new(
            ExprKind::Yield {
                key: Some(Box::new(first)),
                value: Some(Box::new(value)),
            },
            span,
        ));
    }
    Ok(Expr::new(
        ExprKind::Yield {
            key: None,
            value: Some(Box::new(first)),
        },
        span,
    ))
}

/// Advances `pos` by one and wraps the given `ExprKind` and `Span` in a new `Expr`.
fn parse_simple(
    _tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    kind: ExprKind,
) -> Result<Expr, CompileError> {
    *pos += 1;
    Ok(Expr::new(kind, span))
}

/// Parses a unary operator expression. Consumes the operator token, advances `pos`,
/// then recursively parses the inner expression with the given binding power `bp` to
/// enforce precedence. The `ctor` function constructs the target `ExprKind` variant
/// (e.g., `Negate`, `Not`, `BitNot`). Returns the wrapped unary expression.
fn parse_unary(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    ctor: fn(Box<Expr>) -> ExprKind,
    bp: u8,
) -> Result<Expr, CompileError> {
    *pos += 1;
    let inner = parse_expr_bp(tokens, pos, bp)?;
    Ok(Expr::new(ctor(Box::new(inner)), span))
}

/// Parses PHP's unary plus, `+expr`.
///
/// php-src compiles `+expr` as `expr * 1` (`zend_compile_unary_pm`), so that is the lowering:
/// it converts numeric strings and bools the same way, keeps `-0.0`, and throws the same
/// `TypeError` ("Unsupported operand types: array * int") for operands arithmetic rejects. It
/// binds like unary minus, so `+$a ** 2` is `+($a ** 2)`.
fn parse_unary_plus(tokens: &[SpannedToken], pos: &mut usize, span: Span) -> Result<Expr, CompileError> {
    *pos += 1;
    let inner = parse_expr_bp(tokens, pos, 35)?;
    let one = Expr::new(ExprKind::IntLiteral(1), span);
    Ok(Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(inner),
            op: crate::parser::ast::BinOp::Mul,
            right: Box::new(one),
        },
        span,
    ))
}

/// Parses `include`/`include_once`/`require`/`require_once` in expression position.
///
/// PHP gives the include keywords the lowest precedence above `throw`, so the path is a whole
/// expression: `require __DIR__ . '/en.php'` includes the concatenation and
/// `1 + require 'a' . 'b'` is `1 + (require ('a' . 'b'))`. A parenthesized path is an ordinary
/// grouped expression, so `require ('a') . 'b'` includes `'ab'` as PHP does.
///
/// The node is `ExprKind::IncludeValue`, which `crate::resolver` expands into the included
/// file's statements (or rejects with a diagnostic when the position cannot be lowered).
pub(crate) fn parse_include_value(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    let (once, required) = match tokens[*pos].0 {
        Token::Include => (false, false),
        Token::IncludeOnce => (true, false),
        Token::Require => (false, true),
        _ => (true, true),
    };
    *pos += 1;
    let path = parse_expr(tokens, pos)?;
    Ok(Expr::new(
        ExprKind::IncludeValue { path: Box::new(path), once, required },
        span,
    ))
}

/// Parses a `clone` expression. PHP 8.5 makes `clone` a regular function, so a `(` right
/// after the keyword starts an ordinary call on the function named `clone` (`clone($o)`,
/// `clone($o, [...])`) or its first-class callable form (`clone(...)`), sharing the general
/// argument parser. Any other operand keeps the historical unary construct, its binding
/// power, and its `ExprKind::Clone` AST shape.
fn parse_clone(tokens: &[SpannedToken], pos: &mut usize, span: Span) -> Result<Expr, CompileError> {
    if matches!(
        tokens.get(*pos + 1).map(|(token, _)| token),
        Some(Token::LParen)
    ) {
        *pos += 2;
        return parse_function_call_or_callable(tokens, pos, span, Name::unqualified("clone"));
    }
    parse_unary(tokens, pos, span, ExprKind::Clone, 35)
}

/// Parses a prefix `++` or `--` increment/decrement operator.
///
/// A BARE variable takes the dedicated `PreIncrement`/`PreDecrement` node, which names the local
/// directly and is what every checker and optimizer pass understands. Anything else — a property,
/// an array element, a static property, and their nesting — is parsed as an l-value and desugared,
/// because those nodes carry a `String` and have nowhere to put a place.
///
/// The variable has to be checked for a FOLLOWING place suffix, not just for being a variable.
/// `++$o->n` starts with `Token::Variable("o")`, and taking the fast path on that alone consumed
/// the name and left `->n` behind, so the increment landed on the object and the checker rejected
/// it with `Cannot increment/decrement $o of type Object("C")` (issue #682). The same happened to
/// `++$b[0]` and `++$m["k"]`. `++$this->n` escaped it only because `$this` is its own token.
fn parse_prefix_inc_dec(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    increment: bool,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos < tokens.len() {
        if let Token::Variable(name) = &tokens[*pos].0 {
            if !place_suffix_follows(tokens, *pos + 1) {
                let name = name.clone();
                *pos += 1;
                return Ok(Expr::new(
                    if increment {
                        ExprKind::PreIncrement(name)
                    } else {
                        ExprKind::PreDecrement(name)
                    },
                    span,
                ));
            }
        }
    }
    // Not a bare variable: it may still be an l-value the increment node cannot name,
    // such as `++$this->n` or `++$a[0]`. Parsing it as a unary operand and desugaring is
    // purely additive — every input that reaches here failed to parse before, so the
    // original error stands for anything the desugaring declines (a call, for instance,
    // which cannot be read twice).
    let rewind = *pos;
    if let Ok(target) = parse_expr_bp(tokens, pos, 35) {
        super::assignment_targets::reject_dynamic_member_write(&target, span)?;
        if let Some(desugared) =
            super::assignment_targets::desugar_lvalue_incdec(target, increment, true, span)
        {
            return Ok(desugared);
        }
    }
    *pos = rewind;
    Err(CompileError::new(
        span,
        if increment {
            "Expected variable after '++'"
        } else {
            "Expected variable after '--'"
        },
    ))
}

/// Reports whether the token at `index` continues a variable into a larger l-value.
///
/// `->`, `?->`, `::` and `[` are the four ways a place extends past its base variable. Seeing one
/// means the prefix increment's real target is that larger place, not the variable itself.
fn place_suffix_follows(tokens: &[SpannedToken], index: usize) -> bool {
    matches!(
        tokens.get(index).map(|token| &token.0),
        Some(Token::Arrow | Token::QuestionArrow | Token::DoubleColon | Token::LBracket)
    )
}

/// Parses a variable expression starting with a `Variable` token. Consumes the variable name,
/// then checks for a following `++`, `--` (postfix form), or `(` (closure-call syntax like `$var(...)`).
/// Returns `Variable`, `PostIncrement`, `PostDecrement`, or `ClosureCall`. Advances `pos` past
/// any consumed postfix tokens.
fn parse_variable(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    name: String,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos < tokens.len() {
        match &tokens[*pos].0 {
            Token::PlusPlus => {
                *pos += 1;
                return Ok(Expr::new(ExprKind::PostIncrement(name), span));
            }
            Token::MinusMinus => {
                *pos += 1;
                return Ok(Expr::new(ExprKind::PostDecrement(name), span));
            }
            Token::LParen => {
                *pos += 1;
                if super::calls::parse_first_class_callable_parens(tokens, pos)? {
                    // PHP converts the runtime callable itself (`Closure::fromCallable`
                    // semantics): an existing Closure comes back unchanged and a name, a
                    // callable array, or an invokable object becomes a Closure of that target,
                    // which reflection reports as the target rather than as a wrapper.
                    let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
                    return Ok(variable_first_class_callable(name, span));
                }
                let args = parse_args(tokens, pos, span)?;
                let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
                return Ok(Expr::new(ExprKind::ClosureCall { var: name, args }, span));
            }
            _ => {}
        }
    }
    Ok(Expr::new(ExprKind::Variable(name), span))
}

/// Builds the internal Closure conversion behind `$name(...)`.
///
/// The fully qualified name keeps namespace resolution from looking for a namespaced function
/// of the same name.
fn variable_first_class_callable(name: String, span: Span) -> Expr {
    expr_first_class_callable(Expr::new(ExprKind::Variable(name), span), span)
}

/// Builds the internal Closure conversion behind `<expr>(...)` for any runtime callable value:
/// `($this->callback)(...)`, `call()(...)`, `$this(...)`, `$obj->$method(...)` (as the array
/// `[$obj, $method]`).
pub(super) fn expr_first_class_callable(callable: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(
                crate::names::NameKind::FullyQualified,
                vec!["__elephc_closure_from_callable".to_string()],
            ),
            args: vec![callable],
        },
        span,
    )
}

/// Parses a grouped expression `(...)` or a type cast `(type) expr`. If `peek_cast` detects
/// a cast, consumes the cast syntax and returns a `Cast` node with the target type and inner
/// expression parsed at binding power 35 (the unary-operator level). This makes a cast bind
/// tighter than `* / % + - .` and the comparison/logical operators — so `(int)$x + 3` parses
/// as `((int)$x) + 3`, matching PHP — while `**` (left bp 37) still binds tighter than the cast.
/// Otherwise parses as a grouped expression: consumes `(` and `)`, then checks for an immediate
/// call (`inner(args)`) to support expression-call syntax.
fn parse_group_or_cast(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    if let Some(cast_ty) = peek_cast(tokens, *pos) {
        *pos += 3;
        let inner = parse_expr_bp(tokens, pos, 35)?;
        return Ok(Expr::new(
            ExprKind::Cast {
                target: cast_ty,
                expr: Box::new(inner),
            },
            span,
        ));
    }

    *pos += 1;
    let inner = parse_expr(tokens, pos)?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
        return Err(CompileError::new(span, "Expected closing ')'"));
    }
    *pos += 1;
    if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
        let call_span = tokens[*pos].1.span;
        *pos += 1;
        if super::calls::parse_first_class_callable_parens(tokens, pos)? {
            // `($this->callback)(...)`: Closure conversion of the grouped runtime value.
            let call_span = crate::parser::expr::span_through_prev_token(tokens, *pos, call_span);
            return Ok(expr_first_class_callable(inner, call_span));
        }
        let args = parse_args(tokens, pos, call_span)?;
        let call_span = crate::parser::expr::span_through_prev_token(tokens, *pos, call_span);
        return Ok(Expr::new(
            ExprKind::ExprCall {
                callee: Box::new(inner),
                args,
            },
            call_span,
        ));
    }
    Ok(inner)
}

/// Parses a `[...]` array literal.
///
/// Distinguishes indexed (`[a, b]`) and associative (`[key => value]`) forms while
/// preserving leading positional elements that appear before the first keyed entry.
/// Supports spread elements via `...`; spreads in keyed literals are parsed for
/// source-order progress but remain limited by the associative-array representation.
fn parse_array_literal(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    parse_array_literal_with_terminator(tokens, pos, span, &Token::RBracket, "']'")
}

/// Parses the legacy `array(...)` literal form after its opening parenthesis.
pub(super) fn parse_legacy_array_literal(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    parse_array_literal_with_terminator(tokens, pos, span, &Token::RParen, "')'")
}

/// Parses an array literal body up to `closing`, starting at the opening token.
fn parse_array_literal_with_terminator(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    closing: &Token,
    closing_desc: &str,
) -> Result<Expr, CompileError> {
    *pos += 1;
    let mut elems = Vec::new();
    let mut assoc_elems = Vec::new();
    // Non-empty only once a literal is found to mix a spread with an explicit key, which is the
    // one shape neither `ArrayLiteral` nor `ArrayLiteralAssoc` can hold.
    let mut mixed_elems: Vec<ArrayEntry> = Vec::new();
    let mut is_assoc = false;
    let mut first = true;
    let mut next_auto_key = 0i64;
    let mut auto_key_initialized = false;
    // Every element in source order, kept so a literal with by-reference elements (`[&$x]`,
    // `['k' => &$x]`) can be rebuilt as an ordered sequence of writes (see
    // `build_reference_literal`).
    let mut entries: Vec<LiteralEntry> = Vec::new();
    let mut has_reference = false;
    // Whether `next_auto_key` is still exact: false after a spread or a computed key.
    let mut keys_known = true;
    while *pos < tokens.len() && tokens[*pos].0 != *closing {
        if !first {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1.span,
                    "Expected ',' between array elements",
                ));
            }
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == *closing {
                break;
            }
        }
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            let spread_span = tokens[*pos].1.span;
            *pos += 1;
            let inner = parse_expr(tokens, pos)?;
            let spread = Expr::new(ExprKind::Spread(Box::new(inner)), spread_span);
            if is_assoc {
                // The literal already has explicit keys, so it becomes an entry list. The pairs
                // collected so far have to move across with it: leaving them behind is what made
                // `["c" => 8, ...$v]` come out as just the spread. Dropping the spread instead
                // is what made `[...$v, "c" => 8]` come out as just the key (issue #1049).
                migrate_assoc_pairs_to_mixed(&mut assoc_elems, &mut mixed_elems);
                mixed_elems.push(ArrayEntry::Spread(spread.clone()));
            } else {
                elems.push(spread.clone());
            }
            entries.push(LiteralEntry::Spread(spread));
            keys_known = false;
            first = false;
            continue;
        }
        let element_ref = take_reference_array_element(tokens, pos, closing)?;
        let expr = match &element_ref {
            Some(source) => Expr::new(ExprKind::Null, source.span),
            None => parse_expr(tokens, pos)?,
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
            if let Some(source) = &element_ref {
                return Err(CompileError::new(source.span, "An array key cannot be a reference"));
            }
            if !is_assoc {
                promote_indexed_array_items_to_assoc(&mut elems, &mut assoc_elems, &mut mixed_elems);
            }
            is_assoc = true;
            *pos += 1;
            let value_ref = take_reference_array_element(tokens, pos, closing)?;
            let value = match &value_ref {
                Some(source) => Expr::new(ExprKind::Null, source.span),
                None => parse_expr(tokens, pos)?,
            };
            match value_ref {
                Some(source) => {
                    has_reference = true;
                    entries.push(LiteralEntry::Keyed(expr.clone(), LiteralValue::Reference(source)));
                }
                None => entries.push(LiteralEntry::Keyed(expr.clone(), LiteralValue::Value(value.clone()))),
            }
            if explicit_integer_array_key(&expr).is_none()
                && !matches!(expr.kind, ExprKind::StringLiteral(_) | ExprKind::Null)
            {
                keys_known = false;
            }
            update_next_auto_key_from_explicit_key(
                &expr,
                &mut next_auto_key,
                &mut auto_key_initialized,
            );
            if mixed_elems.is_empty() {
                assoc_elems.push((expr, value));
            } else {
                mixed_elems.push(ArrayEntry::Keyed(expr, value));
            }
        } else if is_assoc {
            push_positional_entry(
                &mut entries,
                &mut has_reference,
                element_ref,
                &expr,
                keys_known.then_some(next_auto_key),
            )?;
            if mixed_elems.is_empty() {
                let key = Expr::new(ExprKind::IntLiteral(next_auto_key), expr.span);
                assoc_elems.push((key, expr));
            } else {
                // A bare element after a spread cannot be given a key here: the spread decides
                // how many integer slots it consumed, and only the runtime knows that.
                mixed_elems.push(ArrayEntry::Value(expr));
            }
            next_auto_key += 1;
            auto_key_initialized = true;
        } else {
            push_positional_entry(
                &mut entries,
                &mut has_reference,
                element_ref,
                &expr,
                keys_known.then_some(next_auto_key),
            )?;
            elems.push(expr);
            next_auto_key += 1;
            auto_key_initialized = true;
        }
        first = false;
    }
    if *pos >= tokens.len() || tokens[*pos].0 != *closing {
        return Err(CompileError::new(
            span,
            &format!("Expected {closing_desc}"),
        ));
    }
    *pos += 1;
    let literal = if !mixed_elems.is_empty() {
        Expr::new(ExprKind::ArrayLiteralMixed(mixed_elems), span)
    } else if is_assoc {
        Expr::new(ExprKind::ArrayLiteralAssoc(assoc_elems), span)
    } else {
        Expr::new(ExprKind::ArrayLiteral(elems), span)
    };
    if !has_reference {
        return Ok(literal);
    }
    build_reference_literal(entries, span)
}

/// One element of an array literal, in source order.
enum LiteralEntry {
    /// `V` or `&S` without a key.
    Positional(LiteralValue),
    /// `K => V` or `K => &S`.
    Keyed(Expr, LiteralValue),
    /// `&S` without a key, filed under its exact automatic key.
    PositionalReference(i64, Expr),
    /// `...$s`.
    Spread(Expr),
}

/// The value half of a literal element.
enum LiteralValue {
    Value(Expr),
    Reference(Expr),
}

/// Records a key-less element.
///
/// A by-reference element is filed under its automatic key, which the parser knows exactly
/// (`auto_key`) unless a spread or a computed key came before it.
fn push_positional_entry(
    entries: &mut Vec<LiteralEntry>,
    has_reference: &mut bool,
    reference: Option<Expr>,
    value: &Expr,
    auto_key: Option<i64>,
) -> Result<(), CompileError> {
    match reference {
        Some(source) => {
            let Some(key) = auto_key else {
                return Err(CompileError::new(source.span, UNSUPPORTED_REFERENCE_ELEMENT_KEY));
            };
            *has_reference = true;
            entries.push(LiteralEntry::PositionalReference(key, source));
        }
        None => entries.push(LiteralEntry::Positional(LiteralValue::Value(value.clone()))),
    }
    Ok(())
}

/// Diagnostic for a key-less by-reference element whose automatic key is a runtime fact.
const UNSUPPORTED_REFERENCE_ELEMENT_KEY: &str = "A key-less by-reference array element (`&$x`) is \
     not supported after a spread or a computed key; give it an explicit key";

/// Diagnostic for a by-reference element in a literal that also spreads.
const UNSUPPORTED_REFERENCE_ELEMENT_SPREAD: &str = "A by-reference array element (`&$x`) is not \
     supported in an array literal that also spreads (`...$a`)";

/// Diagnostic for a by-reference element whose source is not a place.
const UNSUPPORTED_REFERENCE_ELEMENT_SOURCE: &str = "A by-reference array element must alias a \
     variable, a property, a static property, an element or a by-reference call";

/// Consumes `&<place>` at an element position and returns the place; `None` when the element is
/// not a reference. `&$this` is returned as `None` with only the `&` consumed: `$this` cannot be
/// reassigned, so PHP wraps a temporary copy of the handle in a reference that only the array
/// holds, and a reference with a single holder is indistinguishable from the value (the first
/// copy of the array dereferences it, and a write through the element never reaches `$this`).
fn take_reference_array_element(
    tokens: &[SpannedToken],
    pos: &mut usize,
    closing: &Token,
) -> Result<Option<Expr>, CompileError> {
    let Some((Token::Ampersand, metadata)) = tokens.get(*pos) else {
        return Ok(None);
    };
    let span = metadata.span;
    *pos += 1;
    if matches!(tokens.get(*pos), Some((Token::This, _)))
        && matches!(tokens.get(*pos + 1), Some((token, _)) if token == &Token::Comma || token == closing)
    {
        return Ok(None);
    }
    let source = parse_expr(tokens, pos)?;
    if !matches!(
        source.kind,
        ExprKind::Variable(_)
            | ExprKind::ArrayAccess { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::FunctionCall { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::StaticMethodCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. }
    ) {
        return Err(CompileError::new(span, UNSUPPORTED_REFERENCE_ELEMENT_SOURCE));
    }
    Ok(Some(source))
}

/// Builds a literal with by-reference elements as the ordered writes PHP performs.
///
/// PHP evaluates each element's key, then its value or reference source, and inserts it before
/// moving on, so a later element observes (and may replace) the earlier ones: a duplicate key
/// overwrites the entry, breaking a reference it held, and `[&$a[$i], $i++]` binds `$a[0]`.
/// The literal therefore becomes, in a hidden table local:
///
/// - `$t[] = V` for a key-less value;
/// - `<break>; $t[K] = V` for a keyed value (a computed `K` is first settled into a hidden
///   local at its own position), where `<break>` rebinds the
///   entry to a fresh cell first (`$t[$k] = &$fresh; unset($fresh)`) whenever an earlier element
///   was a reference whose key it may repeat, so the write replaces the reference instead of
///   writing through it;
/// - `$k = K; $t[$k] = &S` for `K => &S` (key evaluated first, the source bound at its own
///   position); a key-less `&S` uses its automatic key, which the parser tracks exactly.
///
/// The expression's value is the table, MOVED out of the hidden local when the expression is
/// lowered (`ir_lower::expr::assignments`), so no hidden holder outlives the literal.
fn build_reference_literal(entries: Vec<LiteralEntry>, span: Span) -> Result<Expr, CompileError> {
    use crate::parser::ast::{Stmt, StmtKind};
    let stem = format!("__elephc_ref_literal_{}_{}", span.line, span.col);
    let table = crate::names::generated_local_name(&stem);
    let result = crate::names::generated_local_name(&format!("{REFERENCE_LITERAL_VALUE_STEM}{}_{}", span.line, span.col));
    let var = |name: &str, at: Span| Expr::new(ExprKind::Variable(name.to_string()), at);
    let element = |key: Expr, at: Span| {
        Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(var(&table, at)),
                index: Box::new(key),
            },
            at,
        )
    };
    let mut prelude = vec![Stmt::new(
        StmtKind::Assign {
            name: table.clone(),
            value: Expr::new(ExprKind::ArrayLiteral(Vec::new()), span),
        },
        span,
    )];
    let mut temps: Vec<String> = Vec::new();
    // Literal keys bound to a reference so far (normalized), and whether a computed key was.
    let mut reference_keys: Vec<String> = Vec::new();
    let mut computed_reference_key = false;
    let key_temp = |index: usize| crate::names::generated_local_name(&format!("{stem}_k{index}"));
    for (index, entry) in entries.into_iter().enumerate() {
        let temp = |kind: &str, temps: &mut Vec<String>| {
            let name = crate::names::generated_local_name(&format!("{stem}_{kind}{index}"));
            temps.push(name.clone());
            name
        };
        match entry {
            LiteralEntry::Spread(spread) => {
                return Err(CompileError::new(spread.span, UNSUPPORTED_REFERENCE_ELEMENT_SPREAD));
            }
            LiteralEntry::Positional(LiteralValue::Value(value)) => {
                prelude.push(Stmt::new(StmtKind::ArrayPush { array: table.clone(), value }, span));
            }
            LiteralEntry::Keyed(key, LiteralValue::Value(value)) => {
                let normalized = literal_key_identity(&key);
                let key_span = key.span;
                let may_repeat_reference = match &normalized {
                    Some(identity) => computed_reference_key || reference_keys.contains(identity),
                    None => computed_reference_key || !reference_keys.is_empty(),
                };
                // Without a break the write evaluates its own key, then its value, in php's
                // order; with one the key is needed twice, so it is settled first.
                let key = if may_repeat_reference {
                    settle_literal_key(key, key_temp(index), &mut prelude)
                } else {
                    key
                };
                if may_repeat_reference {
                    let fresh = temp("f", &mut temps);
                    // Each synthetic statement that binds or unsets a hidden local sits at its own
                    // source position: local binding decisions are keyed by position.
                    prelude.push(crate::parser::stmt::lower_place_ref_assign(
                        element(key.clone(), key_span),
                        var(&fresh, key_span),
                        key_span,
                    )?);
                    prelude.push(unset_stmt(vec![var(&fresh, value.span)], value.span));
                }
                // The value is evaluated by the write itself, after its (settled) key; the
                // break above touches only the hidden table, which nothing else can observe.
                prelude.push(Stmt::new(
                    StmtKind::ArrayAssign { array: table.clone(), index: key, value },
                    span,
                ));
            }
            LiteralEntry::Keyed(key, LiteralValue::Reference(source)) => {
                match literal_key_identity(&key) {
                    Some(identity) => reference_keys.push(identity),
                    None => computed_reference_key = true,
                }
                let at = source.span;
                let key = settle_literal_key(key, key_temp(index), &mut prelude);
                // The source is bound here, at its own position: a non-variable source goes
                // through a hidden cell local inside `lower_place_ref_assign`.
                prelude.push(crate::parser::stmt::lower_place_ref_assign(element(key, at), source, at)?);
            }
            LiteralEntry::PositionalReference(key, source) => {
                computed_reference_key = true;
                let at = source.span;
                // The automatic key is exact; it is settled into an integer hidden local, which
                // keeps a list-shaped table a list.
                let key = settle_literal_key(Expr::new(ExprKind::IntLiteral(key), at), key_temp(index), &mut prelude);
                prelude.push(crate::parser::stmt::lower_place_ref_assign(element(key, at), source, at)?);
            }
            LiteralEntry::Positional(LiteralValue::Reference(source)) => {
                // `push_positional_entry` files every key-less reference under its key.
                return Err(CompileError::new(source.span, UNSUPPORTED_REFERENCE_ELEMENT_KEY));
            }
        }
    }
    if !temps.is_empty() {
        prelude.push(unset_stmt(temps.iter().map(|name| var(name, span)).collect(), span));
    }
    Ok(Expr::new(
        ExprKind::Assignment {
            target: Box::new(var(&result, span)),
            value: Box::new(var(&table, span)),
            result_target: None,
            prelude,
            conditional_value_temp: None,
        },
        span,
    ))
}

/// Name stem of the result marker of a by-reference array literal; lowering moves the hidden
/// table into the expression's value when it sees it.
pub(crate) const REFERENCE_LITERAL_VALUE_STEM: &str = "__elephc_ref_literal_value_";

/// Returns the key expression a literal element's writes use. A constant key is used as is (it
/// can be evaluated again for free). Any other key is evaluated once, at its own position, into
/// a hidden local the caller unsets afterwards.
///
/// The hidden key local is deliberately NOT unset afterwards: it only ever holds an int or a
/// string (no destructor, no reference), it is released by its next assignment, and unsetting
/// it would re-type its frame slot to `mixed`, which turns every read of it into an owned
/// detached copy that the writes consuming it do not release.
fn settle_literal_key(
    key: Expr,
    name: String,
    prelude: &mut Vec<crate::parser::ast::Stmt>,
) -> Expr {
    // Only a string literal is used as is: an integer key written straight into a still-empty
    // table decides its storage as a hash, which a list-shaped literal must not become.
    if matches!(key.kind, ExprKind::StringLiteral(_)) && explicit_integer_array_key(&key).is_none() {
        return key;
    }
    let key_span = key.span;
    prelude.push(crate::parser::ast::Stmt::new(
        crate::parser::ast::StmtKind::Assign { name: name.clone(), value: key },
        key_span,
    ));
    Expr::new(ExprKind::Variable(name), key_span)
}

/// `unset(...)` as a statement.
fn unset_stmt(args: Vec<Expr>, span: Span) -> crate::parser::ast::Stmt {
    crate::parser::ast::Stmt::new(
        crate::parser::ast::StmtKind::ExprStmt(Expr::new(
            ExprKind::FunctionCall { name: "unset".into(), args },
            span,
        )),
        span,
    )
}

/// The key PHP stores for a literal key, as a comparable string (`'1'`, `1` and `true` are the
/// same key); `None` for a computed key.
fn literal_key_identity(key: &Expr) -> Option<String> {
    if let Some(int) = explicit_integer_array_key(key) {
        return Some(format!("i{int}"));
    }
    match &key.kind {
        ExprKind::StringLiteral(text) => Some(format!("s{text}")),
        ExprKind::Null => Some("s".to_string()),
        _ => None,
    }
}

/// Converts positional items parsed before a keyed array entry into integer-keyed pairs.
/// Moves the already-collected `key => value` pairs into the ordered entry list.
///
/// Called the first time a spread appears in a literal that had already turned associative. The
/// entry list is the only one the node is built from once it is non-empty, so anything left in
/// `assoc_elems` at that point would be silently dropped.
fn migrate_assoc_pairs_to_mixed(
    assoc_elems: &mut Vec<(Expr, Expr)>,
    mixed_elems: &mut Vec<ArrayEntry>,
) {
    for (key, value) in std::mem::take(assoc_elems) {
        mixed_elems.push(ArrayEntry::Keyed(key, value));
    }
}

/// Re-files the items collected so far once the literal turns out to have explicit keys.
///
/// Without a spread among them the pairs go to `assoc_elems` with their automatic keys, which is
/// the common case and keeps the plain associative node. A spread cannot be given a key -- how
/// many integer slots it consumes is a runtime fact -- so its presence moves EVERY item to the
/// ordered entry list instead. Silently skipping it here is what dropped the elements of
/// `[...$v, "c" => 8]` (issue #1049).
fn promote_indexed_array_items_to_assoc(
    elems: &mut Vec<Expr>,
    assoc_elems: &mut Vec<(Expr, Expr)>,
    mixed_elems: &mut Vec<ArrayEntry>,
) {
    let taken = std::mem::take(elems);
    if !taken.iter().any(|elem| matches!(elem.kind, ExprKind::Spread(_))) {
        let mut auto_key = 0i64;
        for elem in taken {
            let key = Expr::new(ExprKind::IntLiteral(auto_key), elem.span);
            assoc_elems.push((key, elem));
            auto_key += 1;
        }
        return;
    }
    for elem in taken {
        if matches!(elem.kind, ExprKind::Spread(_)) {
            mixed_elems.push(ArrayEntry::Spread(elem));
        } else {
            mixed_elems.push(ArrayEntry::Value(elem));
        }
    }
}

/// Advances the automatic integer key cursor after a statically known integer key.
///
/// The first integer-like key seeds the cursor unconditionally so a leading
/// negative key continues from there (PHP 8.3 semantics); later keys only
/// raise it.
fn update_next_auto_key_from_explicit_key(
    key: &Expr,
    next_auto_key: &mut i64,
    auto_key_initialized: &mut bool,
) {
    if let Some(value) = explicit_integer_array_key(key) {
        let candidate = value.saturating_add(1);
        if !*auto_key_initialized || candidate > *next_auto_key {
            *next_auto_key = candidate;
        }
        *auto_key_initialized = true;
    }
}

/// Returns the integer key PHP assigns to an explicit array key literal,
/// covering the int-normalizing forms: bools, integral floats, canonical
/// numeric strings, and negated numeric literals.
fn explicit_integer_array_key(key: &Expr) -> Option<i64> {
    match &key.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::BoolLiteral(value) => Some(i64::from(*value)),
        ExprKind::FloatLiteral(value) => integral_float_array_key(*value),
        ExprKind::StringLiteral(value) => php_integer_string_array_key(value),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg(),
            ExprKind::FloatLiteral(value) => integral_float_array_key(-*value),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the integer key for a float literal PHP casts without truncation.
fn integral_float_array_key(value: f64) -> Option<i64> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    if value < i64::MIN as f64 || value >= i64::MAX as f64 {
        return None;
    }
    Some(value as i64)
}

/// Returns the integer key for a canonical PHP integer string ("0", no
/// leading zeros, no "-0"); other strings stay string keys.
fn php_integer_string_array_key(value: &str) -> Option<i64> {
    if value == "0" {
        return value.parse().ok();
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty()
        || digits.starts_with('0')
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}
