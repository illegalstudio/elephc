//! Purpose:
//! Implements Pratt parsing for PHP infix, postfix, access, call, and assignment expressions.
//! Encodes operator precedence and associativity into binding-power tables.
//!
//! Called from:
//! - `crate::parser::expr::parse_expr()` and recursive expression parsing paths.
//!
//! Key details:
//! - Binding powers must match PHP precedence exactly because downstream passes trust the AST shape.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::Name;
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind, InstanceOfTarget};
use crate::parser::stmt::parse_name;
use crate::span::Span;

use super::assignment_targets::{
    AssignmentExpressionLowerer, is_assignment_expression_target, is_non_local_assignment_target,
};
use super::calls::parse_first_class_callable_parens;
use super::parse_args;
use super::prefix::parse_prefix;
use super::parse_expr;

/// Parses an expression using Pratt parsing, starting with a prefix expression and
/// extending it via infix, postfix, access, call, and assignment operators.
///
/// `min_bp` is the minimum binding power required to continue parsing. Lower
/// binding powers indicate looser binding (e.g., lower precedence). The function
/// returns when it encounters a token whose binding power is below `min_bp` or
/// when no more infix operators apply.
///
/// # Inputs
/// - `tokens`: the token stream
/// - `pos`: current position (updated to the token after the parsed expression)
/// - `min_bp`: minimum binding power threshold
///
/// # Returns
/// The parsed left-hand side expression with all applicable operators applied.
///
/// # Algorithm
/// 1. Parse an initial prefix expression (`parse_prefix`)
/// 2. In the first loop, consume postfix operators: array access (`[]`),
///    object access (`->` / `?->`), and callable invocation (`()`)
/// 3. In the second loop, consume ternary (`? :`), instanceof, assignments
///    (`=`, `+=`, `??=`, etc.), pipe (`|>`), and remaining binary operators
pub(super) fn parse_expr_bp(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    min_bp: u8,
) -> Result<Expr, CompileError> {
    let mut lhs = parse_prefix(tokens, pos)?;

    loop {
        if *pos >= tokens.len() {
            break;
        }

        match &tokens[*pos].0 {
            Token::LBracket => {
                let span = tokens[*pos].1;
                *pos += 1;
                let index = parse_expr(tokens, pos)?;
                if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
                    return Err(CompileError::new(span, "Expected ']'"));
                }
                *pos += 1;
                lhs = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(lhs),
                        index: Box::new(index),
                    },
                    span,
                );
            }
            Token::Arrow | Token::QuestionArrow => {
                let arrow_span = tokens[*pos].1;
                let nullsafe = tokens[*pos].0 == Token::QuestionArrow;
                *pos += 1;
                let member = match parse_object_member(tokens, pos, arrow_span, nullsafe)? {
                    ObjectMember::Named(member_name) => member_name,
                    ObjectMember::Dynamic(property) => {
                        if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                            return Err(CompileError::new(
                                arrow_span,
                                "Dynamic method calls are not supported yet",
                            ));
                        }
                        lhs = Expr::new(
                            if nullsafe {
                                ExprKind::NullsafeDynamicPropertyAccess {
                                    object: Box::new(lhs),
                                    property: Box::new(property),
                                }
                            } else {
                                ExprKind::DynamicPropertyAccess {
                                    object: Box::new(lhs),
                                    property: Box::new(property),
                                }
                            },
                            arrow_span,
                        );
                        continue;
                    }
                };
                let member_name = member;
                if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                    *pos += 1;
                    if parse_first_class_callable_parens(tokens, pos)? {
                        if nullsafe {
                            return Err(CompileError::new(
                                arrow_span,
                                "Cannot combine nullsafe operator with Closure creation",
                            ));
                        }
                        lhs = Expr::new(
                            ExprKind::FirstClassCallable(CallableTarget::Method {
                                object: Box::new(lhs),
                                method: member_name,
                            }),
                            arrow_span,
                        );
                    } else {
                        let args = parse_args(tokens, pos, arrow_span)?;
                        lhs = Expr::new(
                            if nullsafe {
                                ExprKind::NullsafeMethodCall {
                                    object: Box::new(lhs),
                                    method: member_name,
                                    args,
                                }
                            } else {
                                ExprKind::MethodCall {
                                    object: Box::new(lhs),
                                    method: member_name,
                                    args,
                                }
                            },
                            arrow_span,
                        );
                    }
                } else {
                    lhs = Expr::new(
                        if nullsafe {
                            ExprKind::NullsafePropertyAccess {
                                object: Box::new(lhs),
                                property: member_name,
                            }
                        } else {
                            ExprKind::PropertyAccess {
                                object: Box::new(lhs),
                                property: member_name,
                            }
                        },
                        arrow_span,
                    );
                }
            }
            Token::LParen => {
                if matches!(
                    lhs.kind,
                    ExprKind::ArrayAccess { .. }
                        | ExprKind::ExprCall { .. }
                        | ExprKind::ClosureCall { .. }
                        | ExprKind::FunctionCall { .. }
                ) {
                    let call_span = tokens[*pos].1;
                    *pos += 1;
                    let args = parse_args(tokens, pos, call_span)?;
                    lhs = Expr::new(
                        ExprKind::ExprCall {
                            callee: Box::new(lhs),
                            args,
                        },
                        call_span,
                    );
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    loop {
        if *pos >= tokens.len() {
            break;
        }

        if tokens[*pos].0 == Token::Question {
            let ternary_bp = 7;
            if ternary_bp < min_bp {
                break;
            }

            let span = tokens[*pos].1;
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
                *pos += 1;
                let default = parse_expr_bp(tokens, pos, ternary_bp)?;
                lhs = Expr::new(
                    ExprKind::ShortTernary {
                        value: Box::new(lhs),
                        default: Box::new(default),
                    },
                    span,
                );
                continue;
            }

            let then_expr = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() || tokens[*pos].0 != Token::Colon {
                return Err(CompileError::new(span, "Expected ':' in ternary operator"));
            }
            *pos += 1;
            let else_expr = parse_expr_bp(tokens, pos, ternary_bp)?;
            lhs = Expr::new(
                ExprKind::Ternary {
                    condition: Box::new(lhs),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                },
                span,
            );
            continue;
        }

        if tokens[*pos].0 == Token::InstanceOf {
            let instanceof_bp = 35;
            if instanceof_bp < min_bp {
                break;
            }

            let span = tokens[*pos].1;
            *pos += 1;
            let target = parse_instanceof_target(tokens, pos, span)?;
            lhs = Expr::new(
                ExprKind::InstanceOf {
                    value: Box::new(lhs),
                    target,
                },
                span,
            );
            continue;
        }

        if let Some((op, l_bp, r_bp)) = assignment_bp(&tokens[*pos].0) {
            if l_bp < min_bp {
                break;
            }

            if !is_assignment_expression_target(&lhs) {
                return Err(CompileError::new(lhs.span, "Invalid assignment target"));
            }

            let span = tokens[*pos].1;
            *pos += 1;
            let rhs = parse_expr_bp(tokens, pos, r_bp)?;
            if is_non_local_assignment_target(&lhs) {
                let null_coalesce_assign = matches!(op, AssignmentOperator::NullCoalesce);

                let mut lowerer = AssignmentExpressionLowerer::new(span);
                let target = lowerer.stabilize_non_local_target(lhs, &rhs);
                let conditional_value_temp =
                    null_coalesce_assign.then(|| lowerer.reserve_value_temp());
                let rhs = if null_coalesce_assign {
                    rhs
                } else {
                    lowerer.bind_value(&target, rhs)
                };
                let (value, result_target) = match op {
                    AssignmentOperator::Assign => (rhs.clone(), rhs),
                    AssignmentOperator::NullCoalesce => {
                        let value = assignment_value(
                            target.clone(),
                            AssignmentOperator::NullCoalesce,
                            rhs,
                            span,
                        );
                        (value, target.clone())
                    }
                    AssignmentOperator::Compound(op) => {
                        let value = assignment_value(
                            target.clone(),
                            AssignmentOperator::Compound(op),
                            rhs,
                            span,
                        );
                        let result_value = lowerer.bind_result_value(value);
                        (result_value.clone(), result_value)
                    }
                };
                let prelude = lowerer.finish();
                lhs = Expr::new(
                    ExprKind::Assignment {
                        target: Box::new(target.clone()),
                        value: Box::new(value),
                        result_target: Some(Box::new(result_target)),
                        prelude,
                        conditional_value_temp,
                    },
                    span,
                );
            } else {
                let value = match op {
                    AssignmentOperator::Assign => rhs,
                    op => assignment_value(lhs.clone(), op, rhs, span),
                };
                lhs = Expr::new(
                    ExprKind::Assignment {
                        target: Box::new(lhs),
                        value: Box::new(value),
                        result_target: None,
                        prelude: Vec::new(),
                        conditional_value_temp: None,
                    },
                    span,
                );
            }
            continue;
        }

        // PHP 8.5 pipe operator `|>`: left-associative, BP (24, 25) — sits between
        // comparisons (23, 24) and shifts (25, 26), matching php-src/RFC precedence
        // (lower than `.`, shifts, `+`/`-`, higher than comparisons, `??`, ternary,
        // logical, and assignment). Built as a dedicated `ExprKind::Pipe` node, not a
        // `BinOp`, so that LHS-first evaluation order and pipe-specific diagnostics are
        // preserved through later passes.
        if matches!(tokens[*pos].0, Token::PipeArrow) {
            let (l_bp, r_bp) = (24u8, 25u8);
            if l_bp < min_bp {
                break;
            }
            let span = tokens[*pos].1;
            *pos += 1;
            if starts_unparenthesized_arrow_function(tokens, *pos) {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Arrow functions used as pipe targets must be parenthesized",
                ));
            }
            let rhs = parse_expr_bp(tokens, pos, r_bp)?;
            lhs = Expr::new(
                ExprKind::Pipe {
                    value: Box::new(lhs),
                    callable: Box::new(rhs),
                },
                span,
            );
            continue;
        }

        let (op, l_bp, r_bp) = match infix_bp(&tokens[*pos].0) {
            Some(binding) => binding,
            None => break,
        };

        if l_bp < min_bp {
            break;
        }

        let span = tokens[*pos].1;
        *pos += 1;
        let rhs = parse_expr_bp(tokens, pos, r_bp)?;
        if op == BinOp::NullCoalesce {
            lhs = Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(lhs),
                    default: Box::new(rhs),
                },
                span,
            );
        } else {
            lhs = Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(lhs),
                    op,
                    right: Box::new(rhs),
                },
                span,
            );
        }
    }

    Ok(lhs)
}

/// Returns `true` if the token at `pos` is an arrow function (`fn`) or a static
/// arrow function (`static fn`) that is not wrapped in parentheses.
///
/// Used by pipe operator (`|>`) parsing to reject unparenthesized arrow functions
/// as pipe targets, since PHP requires them to be parenthesized.
fn starts_unparenthesized_arrow_function(tokens: &[(Token, Span)], pos: usize) -> bool {
    matches!(tokens.get(pos).map(|(token, _)| token), Some(Token::Fn))
        || (matches!(tokens.get(pos).map(|(token, _)| token), Some(Token::Static))
            && matches!(tokens.get(pos + 1).map(|(token, _)| token), Some(Token::Fn)))
}

/// Represents the member accessed after an object operator (`->` or `?->`).
///
/// `Named` holds a static identifier string. `Dynamic` holds an expression
/// inside braces (`{$expr}`) used for computed property/method names.
enum ObjectMember {
    Named(String),
    Dynamic(Expr),
}

/// Parses the member or property name following an object operator (`->` or `?->`).
///
/// Handles three forms:
/// - Static identifier: `->foo`
/// - Dynamic expression in braces: `->{$expr}`
/// - PHP 8 semi-reserved keywords as member names: any keyword (e.g. `->self`, `->parent`,
///   `->static`, `->class`, `->list`, `->print`) is accepted via the shared bareword mapper.
///
/// Returns `ObjectMember::Named` for identifiers and keywords, or
/// `ObjectMember::Dynamic` for brace-enclosed expressions.
///
/// # Inputs
/// - `arrow_span`: span of the `->` or `?->` token, used for error reporting
/// - `nullsafe`: whether the operator was `?->` (changes error messages)
fn parse_object_member(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    arrow_span: Span,
    nullsafe: bool,
) -> Result<ObjectMember, CompileError> {
    if let Some((Token::LBrace, _)) = tokens.get(*pos) {
        *pos += 1;
        let property = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RBrace {
            return Err(CompileError::new(arrow_span, "Expected '}'"));
        }
        *pos += 1;
        return Ok(ObjectMember::Dynamic(property));
    }
    // PHP 8 allows identifiers and any semi-reserved keyword as a member name after `->`/`?->`.
    if let Some(name) = tokens
        .get(*pos)
        .and_then(|(token, _)| crate::parser::keyword_name::bareword_name_from_token(token))
    {
        *pos += 1;
        return Ok(ObjectMember::Named(name));
    }
    Err(CompileError::new(
        arrow_span,
        if nullsafe {
            "Expected property or method name after '?->'"
        } else {
            "Expected property or method name after '->'"
        },
    ))
}

/// Represents the specific assignment operator encountered during parsing.
#[derive(Debug, Clone, PartialEq)]
enum AssignmentOperator {
    Assign,
    Compound(BinOp),
    NullCoalesce,
}

/// Looks up assignment operator binding power.
///
/// Returns `None` for non-assignment tokens. For recognized tokens, returns
/// `(AssignmentOperator, left_bp, right_bp)` with binding powers `(7, 6)`,
/// enforcing right-associativity (rhs binds tighter than lhs).
fn assignment_bp(token: &Token) -> Option<(AssignmentOperator, u8, u8)> {
    let op = match token {
        Token::Assign => AssignmentOperator::Assign,
        Token::PlusAssign => AssignmentOperator::Compound(BinOp::Add),
        Token::MinusAssign => AssignmentOperator::Compound(BinOp::Sub),
        Token::StarAssign => AssignmentOperator::Compound(BinOp::Mul),
        Token::StarStarAssign => AssignmentOperator::Compound(BinOp::Pow),
        Token::SlashAssign => AssignmentOperator::Compound(BinOp::Div),
        Token::PercentAssign => AssignmentOperator::Compound(BinOp::Mod),
        Token::DotAssign => AssignmentOperator::Compound(BinOp::Concat),
        Token::AmpAssign => AssignmentOperator::Compound(BinOp::BitAnd),
        Token::PipeAssign => AssignmentOperator::Compound(BinOp::BitOr),
        Token::CaretAssign => AssignmentOperator::Compound(BinOp::BitXor),
        Token::LessLessAssign => AssignmentOperator::Compound(BinOp::ShiftLeft),
        Token::GreaterGreaterAssign => AssignmentOperator::Compound(BinOp::ShiftRight),
        Token::QuestionQuestionAssign => AssignmentOperator::NullCoalesce,
        _ => return None,
    };
    Some((op, 7, 6))
}

/// Computes the value expression for an assignment operator applied to `target`
/// and `rhs`.
///
/// - `Assign`: returns `rhs`
/// - `Compound(op)`: returns `BinaryOp { left: target, op, right: rhs }`
/// - `NullCoalesce`: returns `NullCoalesce { value: target, default: rhs }`
///
/// The `target` is consumed and placed on the left side of the derived expression.
fn assignment_value(target: Expr, op: AssignmentOperator, rhs: Expr, span: Span) -> Expr {
    match op {
        AssignmentOperator::Assign => rhs,
        AssignmentOperator::Compound(op) => Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(target),
                op,
                right: Box::new(rhs),
            },
            span,
        ),
        AssignmentOperator::NullCoalesce => Expr::new(
            ExprKind::NullCoalesce {
                value: Box::new(target),
                default: Box::new(rhs),
            },
            span,
        ),
    }
}

/// Parses the target of an `instanceof` operator.
///
/// Handles the PHP 8.0 class-name forms and the dynamic expression form:
/// - `self`, `parent`, `static` keyword → `InstanceOfTarget::Name`
/// - Variable or parenthesized expression → parsed as `Expr`, wrapped in
///   `InstanceOfTarget::Expr` with binding power 36 (above comparisons)
/// - Class/interface name → resolved via `parse_name` into a qualified `Name`
///
/// The dynamic form is parsed with `min_bp = 36` to ensure it captures everything
/// with tighter precedence than comparison operators.
fn parse_instanceof_target(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<InstanceOfTarget, CompileError> {
    match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Self_) => {
            *pos += 1;
            Ok(InstanceOfTarget::Name(Name::unqualified("self")))
        }
        Some(Token::Parent) => {
            *pos += 1;
            Ok(InstanceOfTarget::Name(Name::unqualified("parent")))
        }
        Some(Token::Static) => {
            *pos += 1;
            Ok(InstanceOfTarget::Name(Name::unqualified("static")))
        }
        Some(Token::Variable(_)) | Some(Token::LParen) => {
            let target = parse_expr_bp(tokens, pos, 36)?;
            Ok(InstanceOfTarget::Expr(Box::new(target)))
        }
        _ => parse_name(
            tokens,
            pos,
            span,
            "Expected class or interface name after 'instanceof'",
        )
        .map(InstanceOfTarget::Name),
    }
}

/// Looks up binary operator binding power for Pratt parsing.
///
/// Returns `None` for tokens that are not binary operators. For recognized tokens,
/// returns `(BinOp, left_bp, right_bp)` where the left binding power is used
/// when parsing the left operand and the right binding power when parsing the
/// right operand. The asymmetric pair encodes associativity: left-associative
/// operators have `l_bp < r_bp` so the right operand is parsed with a tighter
/// bp threshold, while right-associative operators (e.g., `**`) have
/// `l_bp > r_bp` so the left side binds tighter.
///
/// Precedence order (lowest to highest): `or` (1) < `xor` (3) < `and` (5)
/// < `??` (9) < `||` (11) < `&&` (13) < `|` (15) < `^` (17) < `&` (19)
/// < `==`/`!=`/`===`/`!==` (21) < `<`/`>`/`<=`/`>=`/`<=>` (23)
/// < `<<`/`>>` (25) < `.` (27) < `+`/`-` (29) < `*`/`/`/`%` (31)
/// < `**` (37 right-assoc)
fn infix_bp(token: &Token) -> Option<(BinOp, u8, u8)> {
    match token {
        Token::Or => Some((BinOp::Or, 1, 2)),
        Token::Xor => Some((BinOp::Xor, 3, 4)),
        Token::And => Some((BinOp::And, 5, 6)),
        Token::QuestionQuestion => Some((BinOp::NullCoalesce, 9, 8)),
        Token::OrOr => Some((BinOp::Or, 11, 12)),
        Token::AndAnd => Some((BinOp::And, 13, 14)),
        Token::Pipe => Some((BinOp::BitOr, 15, 16)),
        Token::Caret => Some((BinOp::BitXor, 17, 18)),
        Token::Ampersand => Some((BinOp::BitAnd, 19, 20)),
        Token::EqualEqual => Some((BinOp::Eq, 21, 22)),
        Token::NotEqual => Some((BinOp::NotEq, 21, 22)),
        Token::EqualEqualEqual => Some((BinOp::StrictEq, 21, 22)),
        Token::NotEqualEqual => Some((BinOp::StrictNotEq, 21, 22)),
        Token::Less => Some((BinOp::Lt, 23, 24)),
        Token::Greater => Some((BinOp::Gt, 23, 24)),
        Token::LessEqual => Some((BinOp::LtEq, 23, 24)),
        Token::GreaterEqual => Some((BinOp::GtEq, 23, 24)),
        Token::Spaceship => Some((BinOp::Spaceship, 23, 24)),
        Token::LessLess => Some((BinOp::ShiftLeft, 25, 26)),
        Token::GreaterGreater => Some((BinOp::ShiftRight, 25, 26)),
        Token::Dot => Some((BinOp::Concat, 27, 28)),
        Token::Plus => Some((BinOp::Add, 29, 30)),
        Token::Minus => Some((BinOp::Sub, 29, 30)),
        Token::Star => Some((BinOp::Mul, 31, 32)),
        Token::Slash => Some((BinOp::Div, 31, 32)),
        Token::Percent => Some((BinOp::Mod, 31, 32)),
        Token::StarStar => Some((BinOp::Pow, 37, 36)),
        _ => None,
    }
}
