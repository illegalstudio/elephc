//! Purpose:
//! Detects and lowers postfix assignments for complex expression targets.
//! Replays parseable l-values and creates effect-preserving lowerings for property/static assignments.
//!
//! Called from:
//! - `crate::parser::stmt::simple::parse_expr_stmt()` and assignment statement dispatch.
//!
//! Key details:
//! - Complex target lowering must not duplicate side effects while preserving PHP source evaluation order.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{
    ARRAY_PROPERTY_REF_TEMP_PREFIX, BinOp, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind,
    NESTED_APPEND_TEMP_PREFIX,
};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;
use super::compound::{
    assignment_operator, assignment_value, is_valid_reference_source, AssignmentOperator,
};

/// Parses a postfix assignment where the target involves property access, array access,
/// or other complex expressions. Detects `+=` append style via `[]` in the target.
/// Returns the lowered `StmtKind` directly for simple targets, or synthesizes a
/// temporary-variable sequence for effectful (compound operator) targets that cannot
/// be replayed safely.
/// Returns `Ok(None)` if the token range does not contain a postfix assignment pattern.
pub(in crate::parser::stmt) fn try_parse_postfix_assignment(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((assign_pos, op)) = find_top_level_assignment(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    let is_append = lhs.len() >= 3
        && lhs[lhs.len() - 2].0 == Token::LBracket
        && lhs[lhs.len() - 1].0 == Token::RBracket;
    if contains_empty_dimension(lhs)
        && (!is_append || op != AssignmentOperator::Assign
            || contains_empty_dimension(&lhs[..lhs.len() - 2]))
    {
        return parse_append_write_statement(tokens, pos, span).map(Some);
    }
    let contains_postfix = lhs
        .iter()
        .skip(1)
        .any(|(token, _)| matches!(token, Token::Arrow | Token::QuestionArrow | Token::LBracket));
    if !contains_postfix {
        return Ok(None);
    }

    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append {
        &lhs[..lhs.len() - 2]
    } else {
        lhs
    };
    let lhs_expr = parse_expr(lhs_expr_tokens, &mut lhs_pos)?;
    if lhs_pos != lhs_expr_tokens.len() {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }
    // `f()[] = $v;` and `$obj->get()['k'] = $v;` write to the call's RESULT. The expression
    // parser binds that result to a hidden local first (php's "evaluate once, write to the
    // value"), so the statement is left to it instead of being refused here.
    if dimension_write_roots_in_call(&lhs_expr, is_append) {
        return Ok(None);
    }

    // Runtime-name writes need the expression lowerer's captured RHS and lazy ??= plan.
    // A handcrafted assignment without its result target evaluated the value twice.
    // `$object->$name = &$value` is a reference binding, which the expression parser only takes
    // into a plain variable: it stays on the statement path below (`dynamic_property_reference`,
    // Symfony's DeepClone hydrator).
    let binds_reference = op == AssignmentOperator::Assign
        && matches!(tokens.get(assign_pos + 1).map(|(token, _)| token), Some(Token::Ampersand));
    if !is_append
        && !binds_reference
        && matches!(lhs_expr.kind, ExprKind::DynamicPropertyAccess { .. })
    {
        let value = parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return Ok(Some(Stmt::new(StmtKind::ExprStmt(value), span)));
    }

    *pos = assign_pos + 1;
    if op == AssignmentOperator::Assign
        && matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Ampersand))
    {
        return parse_complex_ref_assignment(lhs_expr, is_append, tokens, pos, span).map(Some);
    }
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_postfix_assignment(lhs_expr, op, rhs, span).map(Some);
    }
    // A compound write reads its element at store time too, so a right-hand side that
    // reassigns a plain-variable index decides which element is READ as well as which is
    // written: `$c = [10, 20]; $k = 0; $c[$k] += ($k = 1);` leaves `[10, 21]` in PHP. The
    // desugar below embeds that read INSIDE the right-hand side, where it would still see
    // the old index, so the right-hand side is settled into a temporary first.
    //
    // `??=` is deliberately excluded although it is also "not `Assign`": its right-hand
    // side is LAZY, evaluated only when the key is absent. Hoisting it runs it every time
    // AND before the presence check, so `$a[$slot] ??= ($slot = 0)` would test the slot the
    // right-hand side just selected instead of the one written in the source.
    let mut hoisted = EffectfulTargetLowerer::new(span);
    let (lhs_expr, rhs) = if is_append
        && matches!(lhs_expr.kind, ExprKind::PropertyAccess { .. })
        && has_delayed_static_receiver(&lhs_expr)
    {
        // Computed dimensions run before the RHS, but pure property traversal waits until
        // afterwards. A child-property append must not retain the old static object chain.
        let target = hoisted.stabilize_array_base(lhs_expr);
        (target, hoisted.stabilize_unconditionally(rhs))
    } else {
        let rhs = if matches!(op, AssignmentOperator::Compound(_))
            && compound_rhs_can_disturb_index(&lhs_expr, &rhs)
        {
            hoisted.stabilize_unconditionally(rhs)
        } else {
            rhs
        };
        (lhs_expr, rhs)
    };
    // A compound operation reads and writes one dimension. Capture a mutable index after
    // any eager RHS evaluation, before a diagnostic handler can change its source variable.
    let lhs_expr = if matches!(op, AssignmentOperator::Compound(_)) {
        hoisted.snapshot_update_dimension(lhs_expr)
    } else {
        lhs_expr
    };
    let lhs_span = lhs_expr.span;
    if is_append {
        let stmt = match lhs_expr.kind {
            ExprKind::Variable(array) => StmtKind::ArrayPush { array, value: rhs },
            ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayPush {
                object,
                property,
                value: rhs,
            },
            ExprKind::ArrayAccess { array, index } => {
                return lower_nested_append_assignment(
                    Expr::new(ExprKind::ArrayAccess { array, index }, lhs_span),
                    rhs,
                    span,
                )
                .map(Some);
            }
            ExprKind::DynamicPropertyAccess { object, property }
                if matches!(object.kind, ExprKind::This | ExprKind::Variable(_)) =>
            {
                return dynamic_property_append(*object, *property, rhs, span)
                    .map(Some)
                    .ok_or_else(|| CompileError::new(span, "Invalid assignment target"));
            }
            // `$this[] = $v` inside an `ArrayAccess` class is php's `$this->offsetSet(null, $v)`
            // (psysh's Hoa `ProtocolNode`).
            ExprKind::This => {
                return Ok(Some(this_offset_set(Expr::new(ExprKind::Null, lhs_span), rhs, span)));
            }
            _ => return Err(CompileError::new(span, "Invalid assignment target")),
        };
        return Ok(Some(hoisted.finish_if_used(stmt, span)));
    }
    // `$this[$k] = $v`: `$this->offsetSet($k, $v)`.
    if op == AssignmentOperator::Assign {
        if let ExprKind::ArrayAccess { array, index } = &lhs_expr.kind {
            if matches!(array.kind, ExprKind::This) {
                return Ok(Some(this_offset_set((**index).clone(), rhs, span)));
            }
        }
    }

    let value = assignment_value(lhs_expr.clone(), op, rhs, span);

    let stmt = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => StmtKind::ArrayAssign {
                array,
                index: *index,
                value,
            },
            ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                object,
                property,
                index: *index,
                value,
            },
            _ => StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            },
        },
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
            object,
            property,
            value,
        },
        ExprKind::DynamicPropertyAccess { object, property } => StmtKind::ExprStmt(Expr::new(
            ExprKind::Assignment {
                target: Box::new(Expr::new(
                    ExprKind::DynamicPropertyAccess { object, property },
                    lhs_span,
                )),
                value: Box::new(value),
                result_target: None,
                prelude: Vec::new(),
                conditional_value_temp: None,
            },
            span,
        )),
        // `$GLOBALS['name']` tokens look postfix-shaped, but the expression parser rewrites the
        // complete target to one alias variable, so this is an ordinary variable assignment.
        ExprKind::Variable(name) => StmtKind::Assign { name, value },
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };

    Ok(Some(hoisted.finish_if_used(stmt, span)))
}

/// `$this->offsetSet(<offset>, <value>);` -- a dimension write on `$this`, which only an
/// `ArrayAccess` class can receive.
fn this_offset_set(offset: Expr, value: Expr, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::ExprStmt(Expr::new(
            ExprKind::MethodCall {
                object: Box::new(Expr::new(ExprKind::This, span)),
                method: "offsetSet".to_string(),
                args: vec![offset, value],
            },
            span,
        )),
        span,
    )
}

/// Returns true when a dimension write's innermost base is a call result: `f()[k]`,
/// `$o->m()[k][j]`, or — for the append form, whose target is the base itself — `f()`.
fn dimension_write_roots_in_call(target: &Expr, is_append: bool) -> bool {
    if !is_append && !matches!(target.kind, ExprKind::ArrayAccess { .. }) {
        return false;
    }
    let mut base = target;
    while let ExprKind::ArrayAccess { array, .. } = &base.kind {
        base = array;
    }
    matches!(
        base.kind,
        ExprKind::FunctionCall { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::StaticMethodCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. }
    )
}

/// True when `target` writes an element at a plain-VARIABLE index and `rhs` can reassign
/// that variable before the write lands.
///
/// A replay-safe right-hand side has no effects at all, so it can disturb nothing — which is
/// what keeps `$counts[$k]++` and `$sums[$k] += $n` on the existing path, emitting no temporary.
/// Every other right-hand side is settled first.
///
/// The gate used to also require the right-hand side to MENTION the index by name, which was
/// unsound: `$c[$k] += (function() use (&$k) { $k = 1; return 1; })();` answered `10,11` where
/// PHP answers `10,21`, because the mention sits in a closure the call expression does not scan.
/// Widening it costs a temporary on shapes like `$sums[$k] += f()` and costs no correctness:
/// PHP evaluates a compound assignment's right-hand side BEFORE reading the element, so settling
/// it early is what the language does anyway.
fn compound_rhs_can_disturb_index(target: &Expr, rhs: &Expr) -> bool {
    let ExprKind::ArrayAccess { index, .. } = &target.kind else {
        return false;
    };
    if !matches!(index.kind, ExprKind::Variable(_)) {
        return false;
    }
    !can_replay_assignment_target(rhs)
}

/// Lowers a complex target reference bind into ordinary statements.
///
/// For `$obj->prop =& $source`, PHP first writes the source value into the property-owned cell and
/// then aliases the source local to that cell. A local source therefore lowers to a synthetic
/// property assignment plus the existing local reference assignment. A declared-property source
/// retains its own cell identity in `PropertyRefAssign`, allowing both object slots to own the same
/// cell instead of rebinding a temporary and losing the original alias.
/// Array-element targets such as `$values[$key] =& $source` and nested appends such as
/// `$loops[$key][] =& $source` wrap the source in the owning array-reference marker before
/// reusing the ordinary set or nested append/write-back lowering.
fn parse_complex_ref_assignment(
    target: Expr,
    is_append: bool,
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let source = parse_expr(tokens, pos)?;
    if !is_valid_reference_source(&source.kind) {
        return Err(CompileError::new(
            span,
            "Reference assignment source must be a variable, array/property element, or a by-reference call",
        ));
    }
    // A local alias of a static (`$n = &static::$names`) borrows the static's own storage; an
    // array element would have to own a share of it, and static storage is not a refcounted cell
    // (bead elephc-a03.1). A declared property may hold it (`$this->cache = &self::$htmlCache`,
    // Symfony's `CssSelectorConverter`): the static lives for the whole process, and the refcount
    // traffic on its slot is a no-op outside the heap.
    if matches!(&source.kind, ExprKind::StaticPropertyAccess { .. })
        && !matches!(&target.kind, ExprKind::PropertyAccess { .. })
    {
        return Err(CompileError::new(
            span,
            "A property or array element bound by reference to a static property is not supported by the native backend",
        ));
    }
    expect_semicolon(tokens, pos)?;
    let element_target = is_append || matches!(&target.kind, ExprKind::ArrayAccess { .. });
    if element_target && matches!(&source.kind, ExprKind::ArrayAccess { .. }) {
        return element_to_element_reference(target, is_append, source, span);
    }
    complex_ref_store(target, is_append, source, span)
}

/// Lowers `$a[$k] = &$b[$j]` (or `$a[$k][] = &$b[$j]`) through a hidden local bound to the
/// source element.
///
/// `$t = &$b[$j]` makes the source element a reference in place and gives `$t` a share of it;
/// `$a[$k] = &$t` then stores a marker over that SAME reference, so the two elements are one
/// reference, as in PHP. The trailing `unset` ends only the temp's binding: each element keeps
/// its own share, and the next execution of the statement binds a fresh one.
fn element_to_element_reference(
    target: Expr,
    is_append: bool,
    source: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let temp = format!("__elephc_ref_elem_{}_{}", span.line, span.col);
    bound_through_hidden_local(temp, source, span, |temp_expr| {
        complex_ref_store(target, is_append, temp_expr, span)
    })
}

/// Binds the hidden local `temp` to `source` by reference (`$temp = &<source>`), runs the store
/// `store` builds from that local, then ends the local's binding: the places the store linked
/// keep their own shares of the reference.
fn bound_through_hidden_local(
    temp: String,
    source: Expr,
    span: Span,
    store: impl FnOnce(Expr) -> Result<Stmt, CompileError>,
) -> Result<Stmt, CompileError> {
    let temp_expr = Expr::new(ExprKind::Variable(temp.clone()), span);
    let bind = Stmt::new(
        StmtKind::RefAssign {
            target: temp,
            source,
        },
        span,
    );
    let store = store(temp_expr.clone())?;
    let unbind = Stmt::new(
        StmtKind::ExprStmt(Expr::new(
            ExprKind::FunctionCall {
                name: crate::names::Name::unqualified("unset"),
                args: vec![temp_expr],
            },
            span,
        )),
        span,
    );
    Ok(Stmt::new(StmtKind::Synthetic(vec![bind, store, unbind]), span))
}

/// Lowers `$obj->{'p'}[] = $v` and `$obj->{$c ? 'a' : 'b'}[] = $v` to appends on NAMED
/// properties, the second as an `if`.
///
/// Laravel's `Query\Builder::orderBy()` is `$this->{$this->unions ? 'unionOrders' : 'orders'}[] =
/// [...]`; refusing it sent the whole query builder to the interpreter. A name the parser cannot
/// see is still refused here: a read-append-write through a runtime name would store a boxed
/// array into a property the checker refined, which the backend can only reject at run time.
fn dynamic_property_append(object: Expr, property: Expr, value: Expr, span: Span) -> Option<Stmt> {
    let push = |name: &str, value: Expr| {
        Stmt::new(
            StmtKind::PropertyArrayPush {
                object: Box::new(object.clone()),
                property: name.to_string(),
                value,
            },
            span,
        )
    };
    match property.kind {
        ExprKind::StringLiteral(name) => Some(push(&name, value)),
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            let (ExprKind::StringLiteral(then_name), ExprKind::StringLiteral(else_name)) =
                (&then_expr.kind, &else_expr.kind)
            else {
                return None;
            };
            // The value is evaluated once, after the condition, as php does.
            let temp = format!("__elephc_dynprop_value_{}_{}", span.line, span.col);
            let temp_var = Expr::new(ExprKind::Variable(temp.clone()), span);
            let cond_temp = format!("__elephc_dynprop_cond_{}_{}", span.line, span.col);
            let cond_var = Expr::new(ExprKind::Variable(cond_temp.clone()), span);
            Some(Stmt::new(
                StmtKind::Synthetic(vec![
                    Stmt::new(StmtKind::Assign { name: cond_temp, value: *condition }, span),
                    Stmt::new(StmtKind::Assign { name: temp, value }, span),
                    Stmt::new(
                        StmtKind::If {
                            condition: cond_var,
                            then_body: vec![push(then_name, temp_var.clone())],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![push(else_name, temp_var)]),
                        },
                        span,
                    ),
                ]),
                span,
            ))
        }
        _ => None,
    }
}

/// Builds the store of an already-parsed complex reference assignment.
fn complex_ref_store(
    target: Expr,
    is_append: bool,
    source: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let element_target = is_append || matches!(&target.kind, ExprKind::ArrayAccess { .. });
    let source_is_variable = matches!(&source.kind, ExprKind::Variable(_));
    // `$this->data['forms'][$n] = &$this->build(...)` (Symfony's `FormDataCollector`),
    // `$result[self::REF] = &$object->$name` (`PropertyAccessor`), `$list[] = &$o->p`: the element
    // stores a marker over the reference a hidden local is bound to, which `$t = &<source>`
    // lowers for every source kind.
    if element_target
        && !source_is_variable
        && (is_append || !matches!(&source.kind, ExprKind::PropertyAccess { .. }))
    {
        let prefix = if matches!(&source.kind, ExprKind::PropertyAccess { .. }) {
            ARRAY_PROPERTY_REF_TEMP_PREFIX
        } else {
            "__elephc_ref_elem_"
        };
        let temp = format!("{}{}_{}", prefix, span.line, span.col);
        return bound_through_hidden_local(temp, source, span, |temp_expr| {
            complex_ref_store(target, is_append, temp_expr, span)
        });
    }
    // `$this->in = &$pipes[0]` (Symfony's `ProcessStream`): the element and the property become one
    // reference held in the property's cell. `$t = &$pipes[0]` makes the element a reference (a
    // missing key becomes null without a warning, as php's address-of does), the property takes
    // its value, and the element is re-bound to the property's cell.
    if let (ExprKind::PropertyAccess { object, property }, ExprKind::ArrayAccess { .. }) =
        (&target.kind, &source.kind)
    {
        if can_replay_assignment_target(&source) {
            let temp = format!("__elephc_ref_elem_{}_{}", span.line, span.col);
            let (object, property) = (object.clone(), property.clone());
            return bound_through_hidden_local(temp, source.clone(), span, |temp_expr| {
                let copy = Stmt::new(
                    StmtKind::PropertyAssign {
                        object,
                        property,
                        value: temp_expr,
                    },
                    span,
                );
                let rebind = complex_ref_store(source, false, target, span)?;
                Ok(Stmt::new(StmtKind::Synthetic(vec![copy, rebind]), span))
            });
        }
    }
    if is_append {
        if !matches!(&source.kind, ExprKind::Variable(_)) {
            return Err(CompileError::new(
                span,
                "Nested array reference appends currently require a variable source",
            ));
        }
        let reference = Expr::new(ExprKind::ArrayReference(Box::new(source)), span);
        return lower_nested_append_assignment(target, reference, span);
    }
    let source_name = match &source.kind {
        ExprKind::Variable(source_name) => Some(source_name),
        ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. } => None,
        _ => {
            return Err(CompileError::new(
                span,
                "Complex reference targets currently require a variable or declared-property source",
            ));
        }
    };
    if matches!(&target.kind, ExprKind::ArrayAccess { .. }) {
        if source_name.is_none() {
            // `$values[$key] = &$this->cursor`: the element has to share the property's cell, and
            // the array reference marker only wraps a variable. Bind a hidden local to the
            // property's cell first (the `$tmp = &$obj->prop` form the backend already lowers),
            // then store that local's reference into the element.
            let temp = format!("{}{}_{}", ARRAY_PROPERTY_REF_TEMP_PREFIX, span.line, span.col);
            let bind = Stmt::new(
                StmtKind::RefAssign {
                    target: temp.clone(),
                    source,
                },
                span,
            );
            let reference = Expr::new(
                ExprKind::ArrayReference(Box::new(Expr::new(ExprKind::Variable(temp), span))),
                span,
            );
            let store = Stmt::new(assignment_target_store_stmt(target, reference, span)?, span);
            return Ok(Stmt::new(StmtKind::Synthetic(vec![bind, store]), span));
        }
        let reference = Expr::new(ExprKind::ArrayReference(Box::new(source)), span);
        let kind = assignment_target_store_stmt(target, reference, span)?;
        return Ok(Stmt::new(kind, span));
    }
    if let ExprKind::DynamicPropertyAccess { object, property } = &target.kind {
        return dynamic_property_reference(target.clone(), object, property, source, span);
    }
    let ExprKind::PropertyAccess { object, property } = &target.kind else {
        return Err(CompileError::new(
            span,
            "Complex reference targets currently require a declared property",
        ));
    };
    if source_name.is_none() {
        return Ok(Stmt::new(
            StmtKind::PropertyRefAssign {
                object: object.clone(),
                property: property.clone(),
                source,
            },
            span,
        ));
    }
    let assign = Stmt::new(
        StmtKind::PropertyAssign {
            object: object.clone(),
            property: property.clone(),
            value: source.clone(),
        },
        span,
    );
    let bind = Stmt::new(
        StmtKind::RefAssign {
            target: source_name.expect("variable reference source checked above").clone(),
            source: target,
        },
        span,
    );
    Ok(Stmt::new(StmtKind::Synthetic(vec![assign, bind]), span))
}

/// Lowers `$object->$name = &$source`: the property becomes the reference `$source` is.
///
/// The name is only known at run time, and so, often, is the class (Symfony's DeepClone hydrates
/// objects of any class through `$object->$name = &$value`). The write stores the marker over
/// `$source`'s reference cell through the ordinary runtime-named property store, exactly as
/// `$array[$key] = &$source` stores it in an element: a dynamic property or an untyped declared
/// one holds the marker, and reads follow it. A typed declared property cannot hold one and keeps
/// the value it is assigned.
fn dynamic_property_reference(
    target: Expr,
    _object: &Expr,
    _property: &Expr,
    source: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    // A property or element source (`$data->{$key} = &$this->closure`, laravel/serializable-
    // closure's `Native::mapPointers()`) takes the element-marker route array literals use
    // (`assoc_array_literals::lower_element_reference_marker`).
    if !matches!(
        &source.kind,
        ExprKind::Variable(_) | ExprKind::PropertyAccess { .. } | ExprKind::ArrayAccess { .. }
    ) {
        return Err(CompileError::new(
            span,
            "Runtime-named property reference targets currently require a variable source",
        ));
    }
    Ok(Stmt::new(
        StmtKind::ExprStmt(Expr::new(
            ExprKind::Assignment {
                target: Box::new(target),
                value: Box::new(Expr::new(ExprKind::ArrayReference(Box::new(source)), span)),
                result_target: None,
                prelude: Vec::new(),
                conditional_value_temp: None,
            },
            span,
        )),
        span,
    ))
}

/// Lowers an append through a nested array target (`$a[0][] = $value`) into a
/// synthetic read/append/write-back sequence. The temporary append triggers the
/// existing copy-on-write split, and the final assignment stores the detached
/// nested array back into the original slot.
pub(crate) fn lower_nested_append_assignment(
    target: Expr,
    value: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let delayed_receiver = has_delayed_static_receiver(&target);
    let (target, dynamic_root) = lowerer.hoist_dynamic_property_root(target);
    // The fused-append recognizer (`ir_lower::stmt::nested_append::recognize`) matches the
    // read/push/write-back triple at the END of its own `Synthetic` group, so a hoisted root's
    // extra write-back must sit outside it.
    let group_start = lowerer.stmts.len();
    let target = lowerer.stabilize_array_target(target);
    let value = if delayed_receiver {
        lowerer.stabilize_unconditionally(value)
    } else {
        value
    };
    let temp = lowerer.next_nested_append_temp_name();
    lowerer.stmts.push(Stmt::new(
        StmtKind::Assign {
            name: temp.clone(),
            value: target.clone(),
        },
        span,
    ));
    lowerer.stmts.push(Stmt::new(
        StmtKind::ArrayPush {
            array: temp.clone(),
            value,
        },
        span,
    ));
    let write_back =
        assignment_target_store_stmt(target, Expr::new(ExprKind::Variable(temp), span), span)?;
    if let Some(root) = dynamic_root {
        let mut group = lowerer.stmts.split_off(group_start);
        group.push(Stmt::new(write_back, span));
        lowerer.stmts.push(Stmt::new(StmtKind::Synthetic(group), span));
        return Ok(lowerer.finish(root.write_back(span)));
    }
    Ok(lowerer.finish(write_back))
}

/// A `$object->{$name}` chain root the nested-write desugar read into a temporary.
struct HoistedDynamicPropertyRoot {
    object: Expr,
    name: Expr,
    temp: String,
}

impl HoistedDynamicPropertyRoot {
    /// Stores the (modified) temporary back through the same object and property name.
    fn write_back(self, span: Span) -> StmtKind {
        StmtKind::ExprStmt(Expr::new(
            ExprKind::Assignment {
                target: Box::new(Expr::new(
                    ExprKind::DynamicPropertyAccess {
                        object: Box::new(self.object),
                        property: Box::new(self.name),
                    },
                    span,
                )),
                value: Box::new(Expr::new(ExprKind::Variable(self.temp), span)),
                result_target: None,
                prelude: Vec::new(),
                conditional_value_temp: None,
            },
            span,
        ))
    }
}

/// Builds the statement that writes `value` back into an already-stabilized
/// assignment target. Supports the same local, property, static property, and
/// array target families as postfix assignment lowering.
pub(crate) fn assignment_target_store_stmt(
    target: Expr,
    value: Expr,
    span: Span,
) -> Result<StmtKind, CompileError> {
    match target.kind {
        ExprKind::Variable(name) => Ok(StmtKind::Assign { name, value }),
        ExprKind::PropertyAccess { object, property } => Ok(StmtKind::PropertyAssign {
            object,
            property,
            value,
        }),
        ExprKind::StaticPropertyAccess { receiver, property } => {
            Ok(StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            })
        }
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => Ok(StmtKind::ArrayAssign {
                array,
                index: *index,
                value,
            }),
            ExprKind::PropertyAccess { object, property } => Ok(StmtKind::PropertyArrayAssign {
                object,
                property,
                index: *index,
                value,
            }),
            ExprKind::StaticPropertyAccess { receiver, property } => {
                Ok(StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                })
            }
            _ => Ok(StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            }),
        },
        _ => Err(CompileError::new(span, "Invalid assignment target")),
    }
}

/// Builds the statement that appends `value` through an assignment-expression target.
///
/// Bare locals and property/static-property receivers map to their ordinary push statements;
/// nested array targets reuse the established read/push/write-back desugaring so growth is
/// republished all the way to the original container.
pub(crate) fn assignment_target_append_stmt(
    target: Expr,
    value: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let kind = match target.kind {
        ExprKind::Variable(array) => StmtKind::ArrayPush { array, value },
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            }
        }
        ExprKind::DynamicStaticPropertyAccess { receiver, property } => {
            StmtKind::DynamicStaticPropertyWrite {
                receiver,
                property,
                index: None,
                append: true,
                value,
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            return lower_nested_append_assignment(
                Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
                span,
            );
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(Stmt::new(kind, span))
}

/// Parses discarded post-increment/decrement on a scoped (static class member) l-value target.
/// Handles `A::$x++`, `static::$x--`, `parent::$y++`, etc. Returns `Ok(None)` when no
/// postfix `++`/`--` is found at the top level of the statement.
pub(in crate::parser::stmt) fn try_parse_scoped_postfix_incdec(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((incdec_pos, is_increment)) = find_top_level_postfix_incdec(tokens, start) else {
        return Ok(None);
    };
    if incdec_pos < start + 3 {
        return Ok(None);
    }
    if assignment_precedes(tokens, start, incdec_pos) {
        return Ok(None);
    }

    let lhs = &tokens[start..incdec_pos];
    if contains_empty_dimension(lhs) {
        return parse_append_write_statement(tokens, pos, span).map(Some);
    }
    let mut lhs_pos = 0;
    let lhs_expr = parse_expr(lhs, &mut lhs_pos)?;
    if lhs_pos != lhs.len() {
        return Err(CompileError::new(span, "Invalid increment target"));
    }

    if !matches!(lhs_expr.kind, ExprKind::StaticPropertyAccess { .. }) {
        return Ok(None);
    }

    *pos = incdec_pos + 1;
    expect_semicolon(tokens, pos)?;

    lower_postfix_incdec_assignment(lhs_expr, is_increment, span).map(Some)
}

/// Parses discarded post-increment/decrement on a complex l-value target.
///
/// For statement contexts the original expression result is unused, so `$a[0]++`
/// can be lowered to the same read-modify-write shape as `$a[0] += 1`.
/// Simple local `$x++` is left to the existing local-variable parser.
pub(in crate::parser::stmt) fn try_parse_postfix_incdec(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((incdec_pos, is_increment)) = find_top_level_postfix_incdec(tokens, start) else {
        return Ok(None);
    };
    if incdec_pos < start + 3 {
        return Ok(None);
    }
    if assignment_precedes(tokens, start, incdec_pos) {
        return Ok(None);
    }

    let lhs = &tokens[start..incdec_pos];
    let contains_complex_target = lhs
        .iter()
        .skip(1)
        .any(|(token, _)| matches!(token, Token::Arrow | Token::QuestionArrow | Token::LBracket));
    if !contains_complex_target {
        return Ok(None);
    }
    if contains_empty_dimension(lhs) {
        return parse_append_write_statement(tokens, pos, span).map(Some);
    }

    let mut lhs_pos = 0;
    let lhs_expr = parse_expr(lhs, &mut lhs_pos)?;
    if lhs_pos != lhs.len() {
        return Err(CompileError::new(span, "Invalid increment target"));
    }

    *pos = incdec_pos + 1;
    expect_semicolon(tokens, pos)?;

    lower_postfix_incdec_assignment(lhs_expr, is_increment, span).map(Some)
}

/// Reports whether a top-level assignment operator comes BEFORE `incdec_pos`.
///
/// When it does, the `++`/`--` belongs to the assignment's right-hand side, not to the statement:
/// `$t += $b[0]++;` increments `$b[0]`, it does not increment `$t += $b[0]`. Both postfix-incdec
/// statement parsers used to claim such a statement anyway, parse the whole `$t += $b[0]` as their
/// target, and reject it with `Invalid assignment target` (issue #682). Declining here hands the
/// statement to the assignment parsers, which lower the increment inside the value expression.
fn assignment_precedes(tokens: &[SpannedToken], start: usize, incdec_pos: usize) -> bool {
    find_top_level_assignment(tokens, start)
        .is_some_and(|(assign_pos, _)| assign_pos < incdec_pos)
}

/// Detects write-only dimensions without substituting an ordinary array index.
fn contains_empty_dimension(tokens: &[SpannedToken]) -> bool {
    tokens.windows(2).any(|pair| pair[0].0 == Token::LBracket && pair[1].0 == Token::RBracket)
}

/// Lets the expression parser choose the fetch context for an append/update statement.
fn parse_append_write_statement(
    tokens: &[SpannedToken], pos: &mut usize, span: Span,
) -> Result<Stmt, CompileError> {
    let value = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::ExprStmt(value), span))
}

/// Parses a scoped (static class member) postfix assignment, handling targets like
/// `$obj::prop`, `$obj::$prop`, and `$obj::prop[]`. Detects `+=` append style via `[]`.
/// For compound operators on static properties that cannot be replayed safely, lowers
/// to a temporary-variable sequence via `lower_effectful_static_assignment`.
/// Returns `Ok(None)` when no scoped assignment pattern is found.
pub(in crate::parser::stmt) fn try_parse_scoped_property_assignment(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((assign_pos, op)) = find_top_level_assignment(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    let is_append = lhs.len() >= 3
        && lhs[lhs.len() - 2].0 == Token::LBracket
        && lhs[lhs.len() - 1].0 == Token::RBracket;
    if contains_empty_dimension(lhs)
        && (!is_append || op != AssignmentOperator::Assign
            || contains_empty_dimension(&lhs[..lhs.len() - 2]))
    {
        return parse_append_write_statement(tokens, pos, span).map(Some);
    }
    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append {
        &lhs[..lhs.len() - 2]
    } else {
        lhs
    };
    let lhs_expr = parse_expr(lhs_expr_tokens, &mut lhs_pos)?;
    if lhs_pos != lhs_expr_tokens.len() {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }

    // A static property followed by object access is an instance-property target,
    // not a static store. The shared expression prelude evaluates an effectful RHS before
    // the delayed static receiver fetch, matching PHP instead of retaining a stale object.
    if is_instance_property_assignment_target(&lhs_expr) {
        if !is_append {
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            return Ok(Some(Stmt::new(StmtKind::ExprStmt(value), span)));
        }
        return try_parse_postfix_assignment(tokens, pos, span);
    }

    *pos = assign_pos + 1;
    if op == AssignmentOperator::Assign
        && matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Ampersand))
    {
        *pos += 1;
        let source = parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        let ExprKind::ArrayAccess { array, index } = lhs_expr.kind else {
            return Err(CompileError::new(
                span,
                "Reference assignment target must be a static-property array element",
            ));
        };
        let ExprKind::StaticPropertyAccess { receiver, property } = array.kind else {
            return Err(CompileError::new(
                span,
                "Reference assignment target must be a static-property array element",
            ));
        };
        return Ok(Some(Stmt::new(
            StmtKind::StaticPropertyElementRefAssign {
                receiver,
                property,
                index: *index,
                source,
            },
            span,
        )));
    }
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_static_assignment(lhs_expr, op, rhs, span).map(Some);
    }
    // A compound update settles a disturbing right-hand side, then captures a mutable index,
    // exactly as `try_parse_postfix_assignment` does for local and property targets.
    let mut hoisted = EffectfulTargetLowerer::new(span);
    let (lhs_expr, rhs) = if matches!(op, AssignmentOperator::Compound(_)) {
        let rhs = if matches!(lhs_expr.kind, ExprKind::StaticPropertyAccess { .. })
            || compound_rhs_can_disturb_index(&lhs_expr, &rhs)
        {
            // Bare static reads also follow the RHS, including hooks and exceptions.
            hoisted.stabilize_unconditionally(rhs)
        } else {
            rhs
        };
        (hoisted.snapshot_update_dimension(lhs_expr), rhs)
    } else {
        (lhs_expr, rhs)
    };
    let value = assignment_value(lhs_expr.clone(), op, rhs, span);

    // `self::$b[$k][] = $v` (and its `static::` / `parent::` / `Named::` siblings) is an append
    // through a *nested* target. The trailing `[]` was stripped from the LHS tokens above, so
    // `lhs_expr` is an `ExprKind::ArrayAccess` and the `if is_append` guard on the bare
    // `StaticPropertyAccess` arm below — which only ever handles `self::$b[] = $v` — cannot
    // match it. Left alone it falls into the plain `ArrayAccess` arm, which ignores `is_append`
    // entirely and emits `StaticPropertyArrayAssign`: the append is silently dropped and the
    // bucket is OVERWRITTEN with the single value. Route it through the same read/append/
    // write-back desugar every other nested-append target already uses; the write-back builder
    // (`assignment_target_store_stmt`) already supports the static-property family.
    if is_append && matches!(lhs_expr.kind, ExprKind::ArrayAccess { .. }) {
        return lower_nested_append_assignment(lhs_expr, value, span).map(Some);
    }

    let stmt = match lhs_expr.kind {
        ExprKind::StaticPropertyAccess { receiver, property } if is_append => {
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            }
        }
        ExprKind::DynamicStaticPropertyAccess { receiver, property } if is_append => {
            StmtKind::DynamicStaticPropertyWrite {
                receiver,
                property,
                index: None,
                append: true,
                value,
            }
        }
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                }
            }
            ExprKind::DynamicStaticPropertyAccess { receiver, property } => {
                StmtKind::DynamicStaticPropertyWrite {
                    receiver,
                    property,
                    index: Some(*index),
                    append: false,
                    value,
                }
            }
            _ => StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            },
        },
        ExprKind::StaticPropertyAccess { receiver, property } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        },
        ExprKind::DynamicStaticPropertyAccess { receiver, property } => {
            StmtKind::DynamicStaticPropertyWrite {
                receiver,
                property,
                index: None,
                append: false,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };

    Ok(Some(hoisted.finish_if_used(stmt, span)))
}

/// Recognizes an instance-property write even when one or more array dimensions follow it.
fn is_instance_property_assignment_target(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => is_instance_property_assignment_target(array),
        _ => false,
    }
}

/// Finds a delayed static receiver through literal, runtime-name, and array traversal.
fn has_delayed_static_receiver(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::DynamicPropertyAccess { object, .. } => has_delayed_static_receiver(object),
        ExprKind::ArrayAccess { array, .. } => has_delayed_static_receiver(array),
        _ => false,
    }
}

/// Scans tokens starting from `start` (skipping nested parentheses, brackets, and braces)
/// and returns the first statement-level assignment before a top-level conditional begins.
/// Assignments in ternary or null-coalescing branches remain expression assignments for Pratt.
/// Returns `None` if no eligible operator is found before a semicolon at depth 0.
fn find_top_level_assignment(
    tokens: &[SpannedToken],
    start: usize,
) -> Option<(usize, AssignmentOperator)> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut saw_top_level_conditional = false;
    let mut pos = start;

    while pos < tokens.len() {
        match tokens[pos].0 {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            Token::Question if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                // An assignment after a top-level ternary marker belongs to a branch, not to
                // the whole expression as a postfix statement target (`cond ?: $obj->p = v`).
                // Let the Pratt parser preserve PHP's assignment precedence in that branch.
                saw_top_level_conditional = true;
            }
            Token::QuestionQuestion
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
            {
                // `??` is right-associative and its fallback may be an assignment:
                // `$a ?? $b ?? $k = $name`. Stealing that `=` would treat the complete
                // coalescing expression as an lvalue and reject valid PHP.
                saw_top_level_conditional = true;
            }
            _ if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                if !saw_top_level_conditional {
                    if let Some(op) = assignment_operator(&tokens[pos].0) {
                        return Some((pos, op));
                    }
                }
            }
            _ => {}
        }
        pos += 1;
    }

    None
}

/// Finds a top-level postfix `++` or `--` immediately before the statement semicolon.
///
/// Nested occurrences inside indexes or call arguments are ignored so expressions
/// such as `$items[$i++] = 1` remain assignment statements with an effectful index. A
/// prefix operator inside an assignment RHS (`$id = 'x'.++$obj->counter`) is not a
/// postfix statement target and must remain with the ordinary assignment parser.
fn find_top_level_postfix_incdec(tokens: &[SpannedToken], start: usize) -> Option<(usize, bool)> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut pos = start;

    while pos < tokens.len() {
        match tokens[pos].0 {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            // `$k = $c->i++;` is an ASSIGNMENT whose right-hand side ends in `++`, not an
            // increment statement targeting `$k = $c->i`. Stopping at a top-level `=` hands
            // it to the expression parser, which desugars the increment in place; without
            // this the whole line was claimed here and rejected as an invalid target.
            Token::Assign if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            // A top-level `?` opens a ternary, so the `++` sits in one of its BRANCHES:
            // `$c ? $a[0]++ : $b;` increments `$a[0]`, it does not increment `$c ? $a[0]`.
            // Claiming it anyway made the scan parse `$c ? $a[0]` as its target and report
            // `Expected ':' in ternary operator` from the middle of a statement the scan had
            // truncated itself (issue #1139). Declining leaves the honest diagnostic, which is
            // that a ternary at statement position is not parsed at all yet (#827, #841).
            // `??` and `?->` are their own tokens, so neither reaches this arm.
            Token::Question if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            Token::PlusPlus
                if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && matches!(tokens.get(pos + 1), Some((Token::Semicolon, _))) =>
            {
                return Some((pos, true));
            }
            Token::MinusMinus
                if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && matches!(tokens.get(pos + 1), Some((Token::Semicolon, _))) =>
            {
                return Some((pos, false));
            }
            _ => {}
        }
        pos += 1;
    }

    None
}

/// Returns `true` if the expression is safe to replay as an l-value in a compound assignment,
/// meaning its value can be read multiple times without observable side effects.
/// Replayable expressions include variables, literals, property/static-property access on
/// replayable bases, and recursively their sub-expressions.
/// Function calls, new[], and most other `ExprKind` variants return `false`.
pub(crate) fn can_replay_assignment_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            can_replay_assignment_target(array) && can_replay_assignment_target(index)
        }
        ExprKind::PropertyAccess { object, .. } => can_replay_assignment_target(object),
        ExprKind::DynamicPropertyAccess { object, property } => {
            can_replay_assignment_target(object) && can_replay_assignment_target(property)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            can_replay_assignment_target(left) && can_replay_assignment_target(right)
        }
        ExprKind::InstanceOf { value, target } => {
            can_replay_assignment_target(value) && can_replay_instanceof_target(target)
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value) => can_replay_assignment_target(value),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            can_replay_assignment_target(value) && can_replay_assignment_target(default)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            can_replay_assignment_target(condition)
                && can_replay_assignment_target(then_expr)
                && can_replay_assignment_target(else_expr)
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => true,
        _ => false,
    }
}

/// Returns whether an update's array base is one whose single dimension gets an index snapshot.
///
/// A local, a property, or a static property is written by the statement itself, so its key
/// can be captured before the read. Deeper places keep their own nested-write handling.
pub(crate) fn update_dimension_base_is_snapshotted(array: &Expr) -> bool {
    matches!(
        array.kind,
        ExprKind::Variable(_)
            | ExprKind::PropertyAccess { .. }
            | ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
    )
}

/// Returns whether an update must capture an index before its read and write halves.
pub(crate) fn update_index_needs_snapshot(index: &Expr) -> bool {
    match &index.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => false,
        ExprKind::Variable(name) if crate::names::is_generated_local_name(name) => false,
        _ => true,
    }
}

/// Recursively checks whether `target` can be used as an r-value in a replay-safe assignment.
fn can_replay_instanceof_target(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => true,
        InstanceOfTarget::Generic(_) => true,
        InstanceOfTarget::Expr(expr) => can_replay_assignment_target(expr),
    }
}

/// Lowers a compound postfix assignment (e.g., `+=`, `-=`) on a non-replayable l-value
/// by extracting sub-expressions into temporary variables so each is evaluated exactly once.
/// Builds a `Synthetic` statement containing the temporaries followed by the final assignment.
fn lower_effectful_postfix_assignment(
    lhs_expr: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lowered = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(ExprKind::Variable(array.clone()), span)),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::ArrayAssign {
                    array,
                    index,
                    value,
                }
            }
            ExprKind::PropertyAccess { object, property } => {
                let object = Box::new(lowerer.stabilize(*object));
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::PropertyAccess {
                                object: object.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::PropertyArrayAssign {
                    object,
                    property,
                    index,
                    value,
                }
            }
            _ => {
                let target = lowerer.stabilize_array_target(Expr::new(
                    ExprKind::ArrayAccess { array, index },
                    span,
                ));
                let value = assignment_value(target.clone(), op, rhs, span);
                StmtKind::NestedArrayAssign { target, value }
            }
        },
        ExprKind::PropertyAccess { object, property } => {
            let object = Box::new(lowerer.stabilize(*object));
            let target = Expr::new(
                ExprKind::PropertyAccess {
                    object: object.clone(),
                    property: property.clone(),
                },
                span,
            );
            let value = assignment_value(target, op, rhs, span);
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(lowerer.finish(lowered))
}

/// Lowers discarded post-increment/decrement to the existing assignment statement forms.
///
/// Statement position discards the operator's value, so prefix `++$obj->n;` lowers through
/// here too. Array-element places retain the shared PHP incdec kernel instead of numeric `+= 1`.
pub(crate) fn lower_postfix_incdec_assignment(
    lhs_expr: Expr,
    is_increment: bool,
    span: Span,
) -> Result<Stmt, CompileError> {
    let op = if matches!(&lhs_expr.kind, ExprKind::ArrayAccess { .. }) {
        AssignmentOperator::IncDec(is_increment)
    } else if is_increment {
        AssignmentOperator::Compound(BinOp::Add)
    } else {
        AssignmentOperator::Compound(BinOp::Sub)
    };
    let one = Expr::new(ExprKind::IntLiteral(1), span);

    if matches!(&lhs_expr.kind,
        ExprKind::PropertyAccess { object, .. } | ExprKind::DynamicPropertyAccess { object, .. }
            if has_delayed_static_receiver(object))
    {
        // The expression path shares one static receiver between the read and write,
        // including property hooks whose syntactically replayable reads return fresh objects.
        let AssignmentOperator::Compound(op) = op else { unreachable!("incdec is a compound update"); };
        let expression = crate::parser::expr::compound_assignment_expression(lhs_expr, op, one, span);
        return Ok(Stmt::new(StmtKind::ExprStmt(expression), span));
    }

    if !can_replay_assignment_target(&lhs_expr) {
        if matches!(&lhs_expr.kind, ExprKind::ArrayAccess { array, .. }
            if matches!(array.kind, ExprKind::StaticPropertyAccess { .. }))
        {
            return lower_effectful_static_assignment(lhs_expr, op, one, span);
        }
        return lower_effectful_postfix_assignment(lhs_expr, op, one, span);
    }

    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lhs_expr = lowerer.snapshot_update_dimension(lhs_expr);
    let value = assignment_value(lhs_expr.clone(), op, one, span);
    let kind = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => StmtKind::ArrayAssign {
                array,
                index: *index,
                value,
            },
            ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                object,
                property,
                index: *index,
                value,
            },
            // `self::$rand[$i]++` writes the element in the static property's own storage.
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                }
            },
            _ => StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            },
        },
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
            object,
            property,
            value,
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid increment target")),
    };

    Ok(lowerer.finish_if_used(kind, span))
}

/// Lowers a compound static property assignment where the target cannot be replayed safely.
/// Temporaries are created for any sub-expression that could produce observable side effects
/// (e.g., method calls on the object or array accesses). Returns a `Synthetic` statement.
fn lower_effectful_static_assignment(
    lhs_expr: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lowered = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::StaticPropertyAccess {
                                receiver: receiver.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index,
                    value,
                }
            }
            _ => {
                let target = lowerer.stabilize_array_target(Expr::new(
                    ExprKind::ArrayAccess { array, index },
                    span,
                ));
                let value = assignment_value(target.clone(), op, rhs, span);
                StmtKind::NestedArrayAssign { target, value }
            }
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let target = Expr::new(
                ExprKind::StaticPropertyAccess {
                    receiver: receiver.clone(),
                    property: property.clone(),
                },
                span,
            );
            let value = assignment_value(target, op, rhs, span);
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(lowerer.finish(lowered))
}

/// Helper that rewrites complex l-value targets into sequences of temporary-variable
/// assignments so that source evaluation order is preserved and side effects are not duplicated.
struct EffectfulTargetLowerer {
    span: Span,
    next_temp: usize,
    stmts: Vec<Stmt>,
}

impl EffectfulTargetLowerer {
    /// Initializes the lowerer with the source span used for all synthesized statements
    /// and temporary variable names.
    fn new(span: Span) -> Self {
        Self {
            span,
            next_temp: 0,
            stmts: Vec::new(),
        }
    }

    /// If `expr` is replay-safe, returns it unchanged. Otherwise, emits an `Assign`
    /// statement to a uniquely-named temporary and returns a `Variable` reference to it.
    /// Increments `next_temp` to keep temporary names unique across the same statement.
    fn stabilize(&mut self, expr: Expr) -> Expr {
        if can_replay_assignment_target(&expr) {
            return expr;
        }
        let name = self.next_temp_name();
        self.stmts.push(Stmt::new(
            StmtKind::Assign {
                name: name.clone(),
                value: expr,
            },
            self.span,
        ));
        Expr::new(ExprKind::Variable(name), self.span)
    }

    /// Captures a mutable index once for both halves of an update.
    ///
    /// Covers one dimension of a local, property, or static-property array: the read reports a
    /// float key's conversion, and the write must land on that same key even when a diagnostic
    /// handler reassigns the index variable in between.
    fn snapshot_update_dimension(&mut self, target: Expr) -> Expr {
        let span = target.span;
        match target.kind {
            ExprKind::ArrayAccess { array, index }
                if update_dimension_base_is_snapshotted(&array) =>
            {
                let index = if update_index_needs_snapshot(&index) {
                    self.stabilize_unconditionally(*index)
                } else {
                    *index
                };
                Expr::new(ExprKind::ArrayAccess {
                    array,
                    index: Box::new(index),
                }, span)
            }
            kind => Expr::new(kind, span),
        }
    }

    /// Returns a unique synthetic temporary name for this lowered statement.
    fn next_temp_name(&mut self) -> String {
        let name = crate::names::generated_local_name(&format!(
            "__elephc_compound_{}_{}_{}",
            self.span.line, self.span.col, self.next_temp
        ));
        self.next_temp += 1;
        name
    }

    /// Mints the temporary that holds the bucket of a nested append, under its own reserved
    /// prefix.
    ///
    /// The prefix is what lets IR lowering recognize a nested-append `Synthetic` group and
    /// lower it as a fused, in-place append instead of the read/copy/write-back it desugars
    /// to here. `next_temp_name`'s prefix is shared with the `.=` / `+=` desugars, which emit
    /// the same statement shapes, so it cannot serve as that signal. PHP source cannot forge
    /// either lock: the name carries `crate::names::GENERATED_LOCAL_MARKER`, whose `#` is not a
    /// legal PHP identifier character, and `StmtKind::Synthetic` has no surface syntax.
    fn next_nested_append_temp_name(&mut self) -> String {
        let name = crate::names::generated_local_name(&format!(
            "{}{}_{}_{}",
            NESTED_APPEND_TEMP_PREFIX, self.span.line, self.span.col, self.next_temp
        ));
        self.next_temp += 1;
        name
    }

    /// Reads the `$object->{$name}` root of an array-access chain into a temporary and returns
    /// the chain rooted at that temporary, plus what the caller must write back.
    ///
    /// Such a root has no store statement of its own for a nested write to target, so the
    /// generic desugar stabilized it like any expression: the append landed in a copy that was
    /// never written back. `$this->{lcfirst($symbol) . 'AttributeConfigurators'}[$name][] =
    /// $callable` -- Symfony's attribute autoconfiguration -- registered nothing. The object and
    /// the name are each evaluated once, before the read, as php does.
    fn hoist_dynamic_property_root(
        &mut self,
        target: Expr,
    ) -> (Expr, Option<HoistedDynamicPropertyRoot>) {
        let span = target.span;
        match target.kind {
            ExprKind::ArrayAccess { array, index } => {
                let (array, root) = self.hoist_dynamic_property_root(*array);
                (
                    Expr::new(
                        ExprKind::ArrayAccess {
                            array: Box::new(array),
                            index,
                        },
                        span,
                    ),
                    root,
                )
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                let object = self.stabilize(*object);
                let name = self.stabilize(*property);
                let temp = self.next_temp_name();
                self.stmts.push(Stmt::new(
                    StmtKind::Assign {
                        name: temp.clone(),
                        value: Expr::new(
                            ExprKind::DynamicPropertyAccess {
                                object: Box::new(object.clone()),
                                property: Box::new(name.clone()),
                            },
                            span,
                        ),
                    },
                    self.span,
                ));
                (
                    Expr::new(ExprKind::Variable(temp.clone()), span),
                    Some(HoistedDynamicPropertyRoot { object, name, temp }),
                )
            }
            kind => (Expr::new(kind, span), None),
        }
    }

    /// Stabilizes an array-access target, recursively stabilizing both the array base
    /// and the index. For simple array bases (Variable, This, StaticPropertyAccess),
    /// the array base is kept as-is; deeper bases are stabilized via `stabilize_array_base`.
    fn stabilize_array_target(&mut self, expr: Expr) -> Expr {
        let span = expr.span;
        match expr.kind {
            ExprKind::ArrayAccess { array, index } => Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(self.stabilize_array_base(*array)),
                    index: Box::new(self.stabilize(*index)),
                },
                span,
            ),
            _ => self.stabilize(expr),
        }
    }

    /// Stabilizes the base of a nested array access chain. Recursively processes
    /// `ArrayAccess`, `PropertyAccess`, and `DynamicPropertyAccess` chains; captures indices before
    /// a later effectful dimension can change their inputs. Bare variables retain
    /// PHP's deferred store-time lookup instead of becoming eager expression reads.
    fn stabilize_array_base(&mut self, expr: Expr) -> Expr {
        let span = expr.span;
        match expr.kind {
            ExprKind::ArrayAccess { array, index } => {
                let array = Box::new(self.stabilize_array_base(*array));
                let index = if update_index_needs_snapshot(&index)
                    && !matches!(index.kind, ExprKind::Variable(_))
                {
                    self.stabilize_unconditionally(*index)
                } else {
                    self.stabilize(*index)
                };
                Expr::new(ExprKind::ArrayAccess { array, index: Box::new(index) }, span)
            }
            ExprKind::PropertyAccess { object, property } => Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(self.stabilize_array_base(*object)),
                    property,
                },
                span,
            ),
            ExprKind::DynamicPropertyAccess { object, property } => Expr::new(
                ExprKind::DynamicPropertyAccess {
                    object: Box::new(self.stabilize_array_base(*object)),
                    property: Box::new(self.stabilize(*property)),
                },
                span,
            ),
            ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => expr,
            _ => self.stabilize(expr),
        }
    }

    /// Appends `final_stmt` as the last statement and wraps the entire sequence
    /// in a `Synthetic` statement node returned as a single `Stmt`.
    fn finish(mut self, final_stmt: StmtKind) -> Stmt {
        self.stmts.push(Stmt::new(final_stmt, self.span));
        Stmt::new(StmtKind::Synthetic(self.stmts), self.span)
    }

    /// Returns `final_stmt` unwrapped when nothing was hoisted, so a caller that only
    /// SOMETIMES needs a temporary does not wrap every other statement in a `Synthetic`
    /// group it has no use for.
    fn finish_if_used(self, final_stmt: StmtKind, span: Span) -> Stmt {
        if self.stmts.is_empty() {
            return Stmt::new(final_stmt, span);
        }
        self.finish(final_stmt)
    }

    /// Emits `expr` into a fresh temporary and returns a reference to it, even when the
    /// expression is replay-safe. `stabilize` exists to make an effectful expression
    /// evaluate ONCE; this exists to make it evaluate EARLY.
    fn stabilize_unconditionally(&mut self, expr: Expr) -> Expr {
        let name = self.next_temp_name();
        self.stmts.push(Stmt::new(
            StmtKind::Assign {
                name: name.clone(),
                value: expr,
            },
            self.span,
        ));
        Expr::new(ExprKind::Variable(name), self.span)
    }
}
