//! Purpose:
//! Parses empty dimensions only after their write or update context is known.
//! Lowers append expressions to existing assignment preludes and runtime array pushes.
//!
//! Called from:
//! - `super::pratt` at an empty postfix dimension.
//!
//! Key details:
//! - Empty dimensions never become null or guessed numeric indices in the AST.
//! - Receiver/key effects precede RHS effects, while appends occur after RHS evaluation.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use super::assignment_targets::{AssignmentExpressionLowerer, is_assignment_expression_target};
use super::pratt::{AssignmentOperator, assignment_bp, parse_expr_bp};

/// Consumes an empty dimension and its assignment, postfix update, or enclosing prefix update.
pub(super) fn parse_append_write(
    base: Expr, tokens: &[SpannedToken], pos: &mut usize, span: Span,
    prefix_increment: bool,
) -> Result<Expr, CompileError> {
    *pos += 1; // The opening bracket was consumed by Pratt; consume its empty close.
    let mut dimensions = Vec::new();
    while matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::LBracket)) {
        let dimension_span = tokens[*pos].1.span;
        *pos += 1;
        let index = if matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::RBracket)) {
            None
        } else { Some(super::parse_expr(tokens, pos)?) };
        if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::RBracket)) {
            return Err(CompileError::new(dimension_span, "Expected ']'"));
        }
        *pos += 1;
        dimensions.push(index);
    }
    let property_suffix = matches!(
        tokens.get(*pos).map(|(token, _)| token), Some(Token::Arrow),
    );
    let (update, rhs) = if property_suffix {
        (AppendUpdate::Assign, Expr::new(ExprKind::Null, span))
    } else if prefix_increment {
        (AppendUpdate::PreIncrement, Expr::new(ExprKind::IntLiteral(1), span))
    } else { match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::PlusPlus) => {
            *pos += 1;
            (AppendUpdate::PostIncrement, Expr::new(ExprKind::Null, span))
        }
        Some(Token::MinusMinus) => return Err(CompileError::new(
            span, "Post-decrement on an append dimension is not supported",
        )),
        Some(token) => {
            let Some((operator, _, right_bp)) = assignment_bp(token) else {
                return Err(CompileError::new(span, "Cannot use [] for reading"));
            };
            let update = match operator {
                AssignmentOperator::Assign => AppendUpdate::Assign,
                AssignmentOperator::Compound(operator) => AppendUpdate::Compound(operator),
                AssignmentOperator::NullCoalesce => {
                    return Err(CompileError::new(span, "Cannot use [] for reading"));
                }
            };
            *pos += 1;
            (update, parse_expr_bp(tokens, pos, right_bp)?)
        }
        None => return Err(CompileError::new(span, "Cannot use [] for reading")),
    } };
    if !is_assignment_expression_target(&base) {
        return Err(CompileError::new(base.span, "Invalid assignment target"));
    }
    let span = span.merge(rhs.span);
    let mut stabilizer = AssignmentExpressionLowerer::new(span);
    let base = stabilizer.stabilize_non_local_target(base, &rhs);
    for index in dimensions.iter_mut().flatten() {
        let literal = matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_) | ExprKind::StringLiteral(_) | ExprKind::Null)
            || matches!(&index.kind, ExprKind::Negate(inner)
                if matches!(inner.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)));
        if !literal && !matches!(index.kind, ExprKind::Variable(_)) {
            *index = stabilizer.bind_result_value(index.clone());
        }
    }
    let marker = property_suffix.then(|| Expr::new(ExprKind::Variable(
        crate::names::generated_local_name(&format!("__elephc_append_property_null_{}_{}", span.line, span.col)),
    ), span));
    let rhs = if property_suffix { rhs } else { stabilizer.bind_result_value(rhs) };
    let mut prelude = stabilizer.finish();
    if let Some(Expr { kind: ExprKind::Variable(name), .. }) = &marker {
        // This boundary separates receiver/key capture from the delayed null append.
        prelude.push(Stmt::new(StmtKind::Assign {
            name: name.clone(), value: Expr::new(ExprKind::Null, span),
        }, span));
    }
    let mut lowerer = AppendLowerer { span, next_temp: 0, prelude, result: None };
    if matches!(update, AppendUpdate::Assign) {
        if !property_suffix && dimensions.iter().flatten().any(|index|
            !matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_)))
        {
            lowerer.assign_published_plain(base, &dimensions, &rhs)?;
        } else {
            let value = lowerer.nested_value(&dimensions, &rhs, &update);
            lowerer.push(base, value)?;
        }
    } else {
        lowerer.update_reserved_slot(base, &dimensions, &rhs, &update)?;
    }
    let result = marker.unwrap_or_else(|| lowerer.result.expect("append leaf binds its result"));
    Ok(Expr::new(ExprKind::Assignment {
        target: Box::new(result.clone()), value: Box::new(result), result_target: None,
        prelude: lowerer.prelude, conditional_value_temp: None,
    }, span))
}

/// The operation that writes a newly appended element, never an existing array value.
enum AppendUpdate { Assign, Compound(BinOp), PreIncrement, PostIncrement }

/// One selector between an appended element and the property write that consumes it.
enum AppendChainStep {
    /// A named `->property` member.
    Property(String),
    /// A computed `->{expr}` member.
    DynamicProperty(Expr),
    /// A `[expr]` dimension.
    Index(Expr),
}

/// Finds a property/dimension chain rooted in an implicit null append.
///
/// The chain may include indexed members (`$x[]->a[0]->b`) because PHP autovivifies each
/// step on the value the append just produced. A chain that is only read is rejected by
/// the caller; this helper deliberately does not assign a type to the append result.
fn null_append_property_chain(target: &Expr) -> Option<(&Expr, Vec<AppendChainStep>)> {
    let mut receiver = target;
    let mut steps = Vec::new();
    loop {
        match &receiver.kind {
            ExprKind::PropertyAccess { object, property } => {
                steps.push(AppendChainStep::Property(property.clone()));
                receiver = object;
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                steps.push(AppendChainStep::DynamicProperty((**property).clone()));
                receiver = object;
            }
            ExprKind::ArrayAccess { array, index } => {
                steps.push(AppendChainStep::Index((**index).clone()));
                receiver = array;
            }
            ExprKind::Assignment { target, .. } if matches!(&target.kind,
                ExprKind::Variable(name) if name.starts_with("__elephc_append_property_null_")) => {
                steps.reverse();
                return (!steps.is_empty()).then_some((receiver, steps));
            }
            _ => return None,
        }
    }
}

/// Identifies a dangling append-property read so only genuine write contexts accept it.
pub(super) fn is_null_append_property(target: &Expr) -> bool {
    null_append_property_chain(target).is_some()
}

/// Recognizes the implicit null receiver before a property suffix has been attached.
pub(super) fn is_null_append_receiver(target: &Expr) -> bool {
    matches!(&target.kind, ExprKind::Assignment { target, .. } if matches!(&target.kind,
        ExprKind::Variable(name) if name.starts_with("__elephc_append_property_null_")))
}

/// A chain step whose computed selector has been captured into a compiler local.
enum BoundAppendStep {
    Property(String),
    DynamicProperty(Expr),
    Index(Expr),
}

/// Binds one selector before the append so its side effects precede the new child.
fn bind_append_selector(prelude: &mut Vec<Stmt>, value: Expr, span: Span, index: usize) -> Expr {
    let name = crate::names::generated_local_name(&format!(
        "__elephc_append_property_selector_{}_{}_{}", span.line, span.col, index,
    ));
    prelude.push(Stmt::new(StmtKind::Assign { name: name.clone(), value }, span));
    Expr::new(ExprKind::Variable(name), span)
}

/// Builds a globally resolved helper call with a source-independent span.
fn append_helper_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(ExprKind::FunctionCall {
        name: crate::names::Name::from_parts(
            crate::names::NameKind::FullyQualified, vec![name.to_string()],
        ),
        args,
    }, Span::synthetic())
}

/// Returns the container expression of the append statement at the end of `statements`.
fn append_container(statements: &[Stmt]) -> Option<Expr> {
    statements.iter().rev().find_map(|statement| match &statement.kind {
        StmtKind::PropertyArrayPush { object, property, .. } => Some(Expr::new(
            ExprKind::PropertyAccess {
                object: object.clone(),
                property: property.clone(),
            },
            statement.span,
        )),
        StmtKind::ArrayPush { array, .. } => Some(Expr::new(
            ExprKind::Variable(array.clone()),
            statement.span,
        )),
        StmtKind::StaticPropertyArrayPush { receiver, property, .. } => Some(Expr::new(
            ExprKind::StaticPropertyAccess {
                receiver: receiver.clone(),
                property: property.clone(),
            },
            statement.span,
        )),
        _ => None,
    })
}

/// Selects the element an append just created from its post-append container.
///
/// Arrays expose their final key through `array_key_last`; countable objects such as
/// SimpleXML expose it as `count($container) - 1`. The result is `Mixed` so the caller's
/// property write decides at run time whether it targets an object or throws PHP's
/// "on null" Error for an array's appended null.
fn appended_element(prelude: &mut Vec<Stmt>, container: Expr, span: Span) -> Expr {
    let is_array = append_helper_call("is_array", vec![container.clone()]);
    // The guard proves the cast is only evaluated for arrays; spelling it lets the
    // checker accept `array_key_last` without knowing the runtime container type.
    let array_view = Expr::new(ExprKind::Cast {
        target: CastType::Array,
        expr: Box::new(container.clone()),
    }, span);
    let last_key = append_helper_call("array_key_last", vec![array_view]);
    let count = append_helper_call("count", vec![container.clone()]);
    let last_index = Expr::new(ExprKind::BinaryOp {
        left: Box::new(count),
        op: BinOp::Sub,
        right: Box::new(Expr::new(ExprKind::IntLiteral(1), span)),
    }, span);
    let index = Expr::new(ExprKind::Ternary {
        condition: Box::new(is_array),
        then_expr: Box::new(last_key),
        else_expr: Box::new(last_index),
    }, span);
    let selected = append_helper_call("__elephc_append_element", vec![container, index]);
    let name = crate::names::generated_local_name(&format!(
        "__elephc_append_property_result_{}_{}", span.line, span.col,
    ));
    prelude.push(Stmt::new(StmtKind::Assign { name: name.clone(), value: selected }, span));
    Expr::new(ExprKind::Variable(name), span)
}

/// Builds the lvalue that writes through the appended element and the captured chain.
fn appended_write_target(mut value: Expr, steps: Vec<BoundAppendStep>, span: Span) -> Expr {
    for step in steps {
        value = match step {
            BoundAppendStep::Property(property) => Expr::new(ExprKind::PropertyAccess {
                object: Box::new(value),
                property,
            }, span),
            BoundAppendStep::DynamicProperty(property) => Expr::new(ExprKind::DynamicPropertyAccess {
                object: Box::new(value),
                property: Box::new(property),
            }, span),
            BoundAppendStep::Index(index) => Expr::new(ExprKind::ArrayAccess {
                array: Box::new(value),
                index: Box::new(index),
            }, span),
        };
    }
    value
}

/// Preserves receiver, selector and RHS effects before appending null, then writes through
/// the appended element.
///
/// Arrays append a null element, so the generated write reaches PHP's catchable
/// "Attempt to ... property ... on null" Error. SimpleXML appends an empty child view, so
/// the same write autovivifies and stores the requested value. Increment/decrement keeps the
/// dedicated error path until its prefix/postfix result contract is shared with this helper.
pub(super) fn lower_null_property_write(target: &Expr, rhs: Option<&Expr>, span: Span) -> Option<Expr> {
    let (root, steps) = null_append_property_chain(target)?;
    let ExprKind::Assignment { target: marker, prelude: append, .. } = &root.kind else { return None; };
    let ExprKind::Variable(marker_name) = &marker.kind else { return None; };
    let boundary = append.iter().position(|statement| matches!(&statement.kind,
        StmtKind::Assign { name, .. } if name == marker_name))?;
    let mut prelude = append[..boundary].to_vec();
    let mut bound_steps = Vec::with_capacity(steps.len());
    for (position, step) in steps.into_iter().enumerate() {
        bound_steps.push(match step {
            AppendChainStep::Property(property) => BoundAppendStep::Property(property),
            AppendChainStep::DynamicProperty(property) => {
                BoundAppendStep::DynamicProperty(bind_append_selector(&mut prelude, property, span, position))
            }
            AppendChainStep::Index(index) => {
                BoundAppendStep::Index(bind_append_selector(&mut prelude, index, span, position))
            }
        });
    }
    let rhs_temp = rhs.map(|rhs| {
        let name = crate::names::generated_local_name(&format!(
            "__elephc_append_property_rhs_{}_{}", span.line, span.col,
        ));
        prelude.push(Stmt::new(StmtKind::Assign { name: name.clone(), value: rhs.clone() }, span));
        Expr::new(ExprKind::Variable(name), span)
    });

    let nested = bound_steps.len() > 1;
    let first_property = bound_steps.iter().find_map(|step| match step {
        BoundAppendStep::Property(name) => Some(Expr::new(ExprKind::StringLiteral(name.clone()), span)),
        BoundAppendStep::DynamicProperty(value) | BoundAppendStep::Index(value) => Some(value.clone()),
    });
    let action = if nested { "modify" } else if rhs_temp.is_some() { "assign" } else { "increment/decrement" };
    let message = Expr::new(ExprKind::BinaryOp {
        left: Box::new(Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(ExprKind::StringLiteral(
                format!("Attempt to {action} property \""),
            ), span)),
            op: BinOp::Concat,
            right: Box::new(first_property?),
        }, span)),
        op: BinOp::Concat,
        right: Box::new(Expr::new(ExprKind::StringLiteral("\" on null".to_string()), span)),
    }, span);
    let error = Expr::new(ExprKind::NewObject {
        class_name: crate::names::Name::from_parts(
            crate::names::NameKind::FullyQualified, vec!["Error".to_string()],
        ), args: vec![message],
    }, span);

    let append_statements = append[boundary..].to_vec();
    if let Some(rhs_temp) = rhs_temp {
        if let Some(container) = append_container(&append_statements) {
            let is_array = append_helper_call("is_array", vec![container.clone()]);
            let mut else_body = append_statements.clone();
            let appended = appended_element(&mut else_body, container.clone(), span);
            let lvalue = appended_write_target(appended, bound_steps, span);
            if let Ok(store) = crate::parser::stmt::assignment_target_store_stmt(lvalue, rhs_temp.clone(), span) {
                // A document-root SimpleXMLElement rejects `$sxe[]` before the append: PHP
                // notices, throws "Cannot create unnamed attribute", and leaves the tree
                // unchanged. The runtime probe runs first so no append statement executes on
                // that path; child selections and ordinary objects keep the write-through.
                let root_append = append_helper_call(
                    "__elephc_append_property_root_append",
                    vec![container],
                );
                // Arrays append a null element: release the RHS there before throwing, exactly
                // like the old dedicated error path. Objects (SimpleXML) write through the new
                // child view.
                let array_then = vec![
                    unset_temp(&rhs_temp, span),
                    Stmt::new(StmtKind::Throw(error), span),
                ];
                else_body.push(Stmt::new(StmtKind::If {
                    condition: is_array,
                    then_body: array_then,
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(store, span)]),
                }, span));
                let root_then = vec![
                    unset_temp(&rhs_temp, span),
                    Stmt::new(StmtKind::Throw(Expr::new(ExprKind::NewObject {
                        class_name: crate::names::Name::from_parts(
                            crate::names::NameKind::FullyQualified, vec!["Error".to_string()],
                        ),
                        args: vec![Expr::new(
                            ExprKind::StringLiteral("Cannot create unnamed attribute".to_string()),
                            span,
                        )],
                    }, span)), span),
                ];
                prelude.push(Stmt::new(StmtKind::If {
                    condition: root_append,
                    then_body: root_then,
                    elseif_clauses: Vec::new(),
                    else_body: Some(else_body),
                }, span));
                return Some(Expr::new(ExprKind::Assignment {
                    target: Box::new(rhs_temp.clone()),
                    value: Box::new(rhs_temp),
                    result_target: None,
                    prelude,
                    conditional_value_temp: None,
                }, span));
            }
        }
        // The fallback still performs the append before its catchable property error.
        prelude.extend_from_slice(&append_statements);
        prelude.push(unset_temp(&rhs_temp, span));
        let throw = Expr::new(ExprKind::Throw(Box::new(error)), span);
        let dummy_name = crate::names::generated_local_name(&format!(
            "__elephc_append_property_unreachable_{}_{}", span.line, span.col,
        ));
        let dummy = Expr::new(ExprKind::Variable(dummy_name), span);
        return Some(Expr::new(ExprKind::Assignment {
            target: Box::new(dummy.clone()),
            value: Box::new(throw),
            result_target: None,
            prelude,
            conditional_value_temp: None,
        }, span));
    }

    // Increment/decrement paths also append the null element before their property failure.
    prelude.extend_from_slice(&append_statements);
    let error_name = crate::names::generated_local_name(&format!(
        "__elephc_append_property_error_{}_{}", span.line, span.col,
    ));
    prelude.push(Stmt::new(StmtKind::Assign { name: error_name.clone(), value: error }, span));
    let error = Expr::new(ExprKind::Variable(error_name), span);
    let error = Expr::new(ExprKind::Assignment {
        target: Box::new(error.clone()), value: Box::new(error), result_target: None,
        prelude, conditional_value_temp: None,
    }, span);
    Some(Expr::new(ExprKind::Throw(Box::new(error)), span))
}

/// Releases one compiler temporary before a catchable append-property error.
fn unset_temp(value: &Expr, span: Span) -> Stmt {
    Stmt::new(StmtKind::ExprStmt(Expr::new(ExprKind::FunctionCall {
        name: crate::names::Name::unqualified("unset"),
        args: vec![value.clone()],
    }, span)), span)
}

/// Builds nested containers and the expression result using reserved compiler locals.
struct AppendLowerer { span: Span, next_temp: usize, prelude: Vec<Stmt>, result: Option<Expr> }

impl AppendLowerer {
    /// Reserves a unique local for one append value or expression result.
    fn next_temp(&mut self) -> Expr {
        let name = crate::names::generated_local_name(&format!(
            "__elephc_append_{}_{}_{}", self.span.line, self.span.col, self.next_temp,
        ));
        self.next_temp += 1;
        Expr::new(ExprKind::Variable(name), self.span)
    }

    /// Reserves a result local whose identity preserves prefix semantics across cancellation.
    fn next_prefix_increment_result(&mut self) -> Expr {
        let name = crate::names::generated_append_prefix_increment_result(
            self.span,
            self.next_temp,
        );
        self.next_temp += 1;
        Expr::new(ExprKind::Variable(name), self.span)
    }

    /// Captures one expression so the write and result never re-evaluate it.
    fn bind(&mut self, value: Expr) -> Expr {
        let target = self.next_temp();
        let ExprKind::Variable(name) = &target.kind else { unreachable!() };
        self.prelude.push(Stmt::new(StmtKind::Assign { name: name.clone(), value }, self.span));
        target
    }

    /// Constructs one fresh nested array without converting an append to an indexed read.
    fn nested_value(&mut self, dimensions: &[Option<Expr>], rhs: &Expr, update: &AppendUpdate) -> Expr {
        let Some((dimension, tail)) = dimensions.split_first() else {
            return self.leaf(Expr::new(ExprKind::Null, self.span), rhs, update);
        };
        let container = self.bind(Expr::new(ExprKind::ArrayLiteral(Vec::new()), self.span));
        if let Some(index) = dimension {
            // The eager RHS is already bound. Capture the update key now, before a missing-key
            // warning can call user code that changes the variable used to select this entry.
            let index = if matches!(update, AppendUpdate::Assign)
                || matches!(index.kind, ExprKind::IntLiteral(_)) { index.clone() }
                else { self.bind(index.clone()) };
            let target = Expr::new(ExprKind::ArrayAccess {
                array: Box::new(container.clone()), index: Box::new(index.clone()),
            }, self.span);
            let value = if tail.is_empty() { self.leaf(target.clone(), rhs, update) }
                else { self.nested_value(tail, rhs, update) };
            let ExprKind::Variable(array) = &container.kind else { unreachable!() };
            self.prelude.push(Stmt::new(StmtKind::ArrayAssign {
                array: array.clone(), index: index.clone(), value,
            }, self.span));
        } else {
            let value = self.nested_value(tail, rhs, update);
            self.push(container.clone(), value).expect("fresh container is an append target");
        }
        container
    }

    /// Applies the leaf operation and records its PHP expression result.
    fn leaf(&mut self, current: Expr, rhs: &Expr, update: &AppendUpdate) -> Expr {
        let value = match update {
            AppendUpdate::Assign => rhs.clone(),
            AppendUpdate::Compound(operator) => {
                let value = Expr::new(ExprKind::BinaryOp {
                    left: Box::new(current), op: operator.clone(), right: Box::new(rhs.clone()),
                }, self.span);
                let result = self.next_temp();
                self.result = Some(result.clone());
                // Keep the read-modify-write expression on the store. EIR lowering can then
                // reuse its diagnosed key, while this inline assignment captures the result.
                return Expr::new(ExprKind::Assignment {
                    target: Box::new(result), value: Box::new(value), result_target: None,
                    prelude: Vec::new(), conditional_value_temp: None,
                }, self.span);
            }
            AppendUpdate::PreIncrement => {
                let value = Expr::new(ExprKind::BinaryOp {
                    left: Box::new(current),
                    op: BinOp::Add,
                    right: Box::new(rhs.clone()),
                }, self.span);
                let result = self.next_prefix_increment_result();
                self.result = Some(result.clone());
                return Expr::new(ExprKind::Assignment {
                    target: Box::new(result),
                    value: Box::new(value),
                    result_target: None,
                    prelude: Vec::new(),
                    conditional_value_temp: None,
                }, self.span);
            }
            AppendUpdate::PostIncrement => {
                let current = self.bind(current);
                let result = self.bind(current.clone());
                let ExprKind::Variable(name) = &current.kind else { unreachable!() };
                self.prelude.push(Stmt::new(StmtKind::Assign {
                    name: name.clone(), value: Expr::new(ExprKind::PreIncrement(name.clone()), self.span),
                }, self.span));
                self.result = Some(result);
                return current;
            }
        };
        let value = self.bind(value);
        self.result = Some(value.clone());
        value
    }

    /// Captures ordinary stored cells so a diagnostic cannot redirect an in-progress update.
    fn update_reserved_slot(
        &mut self, base: Expr, dimensions: &[Option<Expr>], rhs: &Expr, update: &AppendUpdate,
    ) -> Result<(), CompileError> {
        let target = self.reserve_append_slot(base)?;
        let mut cell = if dimensions.is_empty() {
            let alias = self.bind_slot_alias(target, true, false);
            self.reset_slot_alias(&alias, Expr::new(ExprKind::Null, self.span));
            alias
        } else {
            let associative = dimensions.first().and_then(Option::as_ref)
                .is_some_and(crate::types::empty_array_key_requires_hash_storage);
            self.bind_slot_alias(target, false, associative)
        };
        for (position, dimension) in dimensions.iter().enumerate() {
            let leaf = position + 1 == dimensions.len();
            let target = if let Some(index) = dimension {
                let index = if matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_)) {
                    index.clone()
                } else {
                    let captured = self.bind(index.clone());
                    let index = self.normalize_dimension_key(captured)?;
                    self.live_guard(&cell, leaf, true);
                    index
                };
                Expr::new(ExprKind::ArrayAccess {
                    array: Box::new(cell.clone()), index: Box::new(index),
                }, self.span)
            } else {
                self.reserve_append_slot(cell.clone())?
            };
            cell = if leaf {
                // Diagnose before materializing a missing leaf, using the captured parent.
                let current = self.bind(target.clone());
                self.live_guard(&cell, true, false);
                let alias = self.bind_slot_alias(target, true, false);
                // Fetch-for-write installs a child container; restore the diagnosed leaf value
                // before applying the scalar update through that attached cell.
                self.reset_slot_alias(&alias, current.clone());
                self.unset_temps(vec![current]);
                alias
            } else {
                // Read first so every missing intermediate dimension emits its PHP warning.
                // The following fetch-for-write materializes the stable stored child.
                let current = self.bind(target.clone());
                self.live_guard(&cell, false, false);
                let associative = dimensions.get(position + 1).and_then(Option::as_ref)
                    .is_some_and(crate::types::empty_array_key_requires_hash_storage);
                let alias = self.bind_slot_alias(target, false, associative);
                self.unset_temps(vec![current]);
                alias
            };
        }
        let value = self.leaf(cell.clone(), rhs, update);
        let ExprKind::Variable(name) = cell.kind else { unreachable!("cell alias is a local") };
        self.prelude.push(Stmt::new(StmtKind::Assign { name, value }, self.span));
        Ok(())
    }

    /// Keeps the role of a probe explicit without adding an observable read or owner.
    fn live_guard(&mut self, cell: &Expr, leaf: bool, before_read: bool) {
        use crate::names::AppendLiveGuardRole;
        let role = match (leaf, before_read) {
            (false, true) => AppendLiveGuardRole::ParentPre,
            (false, false) => AppendLiveGuardRole::ParentPost,
            (true, true) => AppendLiveGuardRole::FinalPre,
            (true, false) => AppendLiveGuardRole::FinalPost,
        };
        let name = crate::names::generated_append_live_guard(self.span, self.next_temp, role);
        self.next_temp += 1;
        self.prelude.push(Stmt::new(StmtKind::Assign { name, value: cell.clone() }, self.span));
    }

    /// Restores the diagnosed leaf value after a writable fetch autovivifies a null slot.
    fn reset_slot_alias(&mut self, alias: &Expr, value: Expr) {
        let ExprKind::Variable(name) = &alias.kind else { unreachable!("cell alias is a local") };
        self.prelude.push(Stmt::new(StmtKind::Assign { name: name.clone(), value }, self.span));
    }

    /// Publishes plain-write parents before key conversion without reading missing children.
    fn assign_published_plain(
        &mut self, base: Expr, dimensions: &[Option<Expr>], rhs: &Expr,
    ) -> Result<(), CompileError> {
        let target = self.reserve_append_slot(base)?;
        let associative = dimensions.first().and_then(Option::as_ref)
            .is_some_and(crate::types::empty_array_key_requires_hash_storage);
        let mut cell = self.bind_slot_alias(target, false, associative);
        for (position, dimension) in dimensions.iter().enumerate() {
            let target = if let Some(index) = dimension {
                let index = if matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_)) {
                    index.clone()
                } else {
                    let index = self.normalize_dimension_key(index.clone())?;
                    self.live_guard(&cell, position + 1 == dimensions.len(), true);
                    index
                };
                Expr::new(ExprKind::ArrayAccess {
                    array: Box::new(cell), index: Box::new(index),
                }, self.span)
            } else {
                self.reserve_append_slot(cell)?
            };
            let leaf = position + 1 == dimensions.len();
            let associative = dimensions.get(position + 1).and_then(Option::as_ref)
                .is_some_and(crate::types::empty_array_key_requires_hash_storage);
            cell = self.bind_slot_alias(target, leaf, associative);
        }
        let value = self.leaf(cell.clone(), rhs, &AppendUpdate::Assign);
        self.reset_slot_alias(&cell, value);
        Ok(())
    }

    /// Marks a temporary ordinary-cell lease; EIR does not promote it into a PHP reference set.
    fn bind_slot_alias(&mut self, source: Expr, leaf: bool, associative_parent: bool) -> Expr {
        let name = crate::names::generated_append_update_alias(
            self.span, self.next_temp, leaf, associative_parent,
        );
        self.next_temp += 1;
        self.prelude.push(Stmt::new(StmtKind::RefAssign {
            target: name.clone(), source,
        }, self.span));
        Expr::new(ExprKind::Variable(name), self.span)
    }

    /// Reserves a null element and freezes the integer key allocated by the existing append path.
    fn reserve_append_slot(&mut self, base: Expr) -> Result<Expr, CompileError> {
        self.push(base.clone(), Expr::new(ExprKind::Null, self.span))?;
        // Give the edge-key call a borrowed local; an owned nested projection must not escape
        // through its argument lifetime. Retire that local before the operation can run user code.
        let container = self.bind(base.clone());
        let boxed_key = self.bind(self.global_call("array_key_last", vec![container.clone()]));
        let index = self.bind(self.global_call("intval", vec![boxed_key.clone()]));
        self.unset_temps(vec![container, boxed_key]);
        Ok(Expr::new(ExprKind::ArrayAccess {
            array: Box::new(base), index: Box::new(index),
        }, self.span))
    }

    /// Uses the shared array writer once to normalize a key before repeated read/write access.
    fn normalize_dimension_key(&mut self, key: Expr) -> Result<Expr, CompileError> {
        // Start with hash storage already owned by a local, so a throwing key diagnostic cannot
        // abandon an unpublished promotion. Updating the seed itself still returns the same key.
        let seed = Expr::new(ExprKind::StringLiteral(String::new()), self.span);
        let keys = self.bind(Expr::new(ExprKind::ArrayLiteralAssoc(vec![
            (seed, Expr::new(ExprKind::Null, self.span)),
        ]), self.span));
        let target = Expr::new(ExprKind::ArrayAccess {
            array: Box::new(keys.clone()), index: Box::new(key),
        }, self.span);
        self.write_slot(target, Expr::new(ExprKind::Null, self.span))?;
        let normalized = self.bind(self.global_call("array_key_last", vec![keys.clone()]));
        self.unset_temps(vec![keys]);
        Ok(normalized)
    }

    /// Constructs globally resolved calls with distinct identities for checked-call metadata.
    /// Their source-independent spans prevent sibling helpers from sharing cached result types.
    fn global_call(&self, name: &str, args: Vec<Expr>) -> Expr {
        Expr::new(ExprKind::FunctionCall {
            name: crate::names::Name::from_parts(
                crate::names::NameKind::FullyQualified, vec![name.to_string()],
            ),
            args,
        }, Span::synthetic())
    }

    /// Ends helper-local ownership as soon as its value has been copied out.
    fn unset_temps(&mut self, values: Vec<Expr>) {
        self.prelude.push(Stmt::new(
            StmtKind::ExprStmt(self.global_call("unset", values)), self.span,
        ));
    }

    /// Reuses ordinary lvalue stores so nested/property writes preserve COW and ownership.
    fn write_slot(&mut self, target: Expr, value: Expr) -> Result<(), CompileError> {
        let statement = crate::parser::stmt::assignment_target_store_stmt(target, value, self.span)?;
        self.prelude.push(Stmt::new(statement, self.span));
        Ok(())
    }

    /// Uses the existing target-aware append lowering, including nested COW write-back.
    fn push(&mut self, base: Expr, value: Expr) -> Result<(), CompileError> {
        let statement = match base.kind {
            ExprKind::Variable(array) => Stmt::new(StmtKind::ArrayPush { array, value }, self.span),
            ExprKind::PropertyAccess { object, property } => {
                Stmt::new(StmtKind::PropertyArrayPush { object, property, value }, self.span)
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                let ExprKind::StringLiteral(property) = property.kind else {
                    return Err(CompileError::new(base.span, "Append to a runtime property name is not supported"));
                };
                Stmt::new(StmtKind::PropertyArrayPush { object, property, value }, self.span)
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                Stmt::new(StmtKind::StaticPropertyArrayPush { receiver, property, value }, self.span)
            }
            ExprKind::ArrayAccess { .. } => {
                crate::parser::stmt::lower_nested_append_assignment(base, value, self.span)?
            }
            _ => return Err(CompileError::new(base.span, "Invalid assignment target")),
        };
        self.prelude.push(statement);
        Ok(())
    }
}
