//! Purpose:
//! Parses and desugars PHP `goto` statements whose labels introduce terminal fallback tails.
//! Keeps the emitted AST structured by replacing each supported jump with a cloned tail.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt_dispatch()` and executable-scope parsing entry points.
//!
//! Key details:
//! - Three shapes are supported: a backward jump (a loop), a forward jump within one statement list
//!   (segments guarded by a jump selector), and a forward jump to a tail that returns or throws
//!   (a cloned tail). Jumps into loops or across function scopes remain errors, as in PHP.
//! - Parser-only markers use NUL-prefixed strings, which cannot collide with PHP source strings.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

const GOTO_MARKER_PREFIX: &str = "\0elephc-terminal-goto:";
const LABEL_MARKER_PREFIX: &str = "\0elephc-terminal-label:";

/// Returns true when the token at `pos` starts a PHP label (`name:`) at statement position.
pub(super) fn starts_label(tokens: &[SpannedToken], pos: usize) -> bool {
    matches!(tokens.get(pos).map(|(token, _)| token), Some(Token::Identifier(_)))
        && matches!(tokens.get(pos + 1).map(|(token, _)| token), Some(Token::Colon))
}

/// Parses `goto label;` into an internal marker removed before the AST leaves the parser.
pub(super) fn parse_goto(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let name = match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Identifier(name)) => name.clone(),
        _ => return Err(CompileError::new(span, "Expected label name after 'goto'")),
    };
    *pos += 1;
    super::stmt::expect_semicolon(tokens, pos)?;
    Ok(marker_stmt(GOTO_MARKER_PREFIX, &name, span))
}

/// Parses `label:` into an internal marker removed before the AST leaves the parser.
pub(super) fn parse_label(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let name = match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Identifier(name)) => name.clone(),
        _ => return Err(CompileError::new(span, "Expected goto label name")),
    };
    *pos += 2;
    Ok(marker_stmt(LABEL_MARKER_PREFIX, &name, span))
}

/// Rewrites supported gotos within one PHP executable scope without crossing function boundaries.
///
/// Three shapes are supported, tried in this order:
/// 1. A BACKWARD jump — every `goto L` for the label sits inside the label's own tail — becomes a
///    real loop (see [`desugar_restart_loops`]). This is the "restart" idiom.
/// 2. A FORWARD jump within one statement list to a label whose tail falls through becomes a
///    guarded segment of that list (see [`desugar_forward_segments`]).
/// 3. A FORWARD jump to a label whose tail always returns or throws becomes a clone of that tail
///    at the jump site (see [`collect_labels`]).
pub(super) fn desugar_scope(statements: &mut Vec<Stmt>) -> Result<(), CompileError> {
    desugar_restart_loops(statements);
    desugar_forward_segments(statements);
    let mut labels = HashMap::new();
    collect_labels(statements, &[], &mut labels)?;
    rewrite_gotos(statements, &labels)
}

/// Turns each backward `goto` into the loop it actually is.
///
/// PHP's restart idiom — used verbatim by Symfony's `ControllerAttributesListener` — is
///
/// ```php
/// restart:
/// foreach ($items as $item) {
///     if ($changed) { $items = refresh(); goto restart; }
/// }
/// ```
///
/// The tail-cloning strategy [`collect_labels`] uses cannot express this: the tail contains the
/// jump, so cloning it would never terminate, and the tail does not return or throw either. But a
/// jump backwards to a label in the same statement list, taken only from within that label's own
/// tail, IS a loop — nothing more. Wrapping the tail in `while (true) { … break; }` and turning
/// each `goto` into `continue <n>` reproduces it exactly, with no new control-flow machinery:
/// `continue` unwinds the loops the jump sits inside and re-enters the synthetic one, and the
/// trailing `break` gives the fall-through exit the label's end-of-list position already meant.
///
/// `<n>` is one more than the number of `break`-able constructs (loops AND `switch`, which PHP
/// counts in its level arithmetic) between the jump and the tail's own level. Pre-existing
/// `break N` / `continue N` inside the tail are renumbered by the same reasoning: one that already
/// reaches past the tail's level targets a construct OUTSIDE the new `while`, so it must count it.
///
/// Anything this does not recognize is left untouched for [`collect_labels`] to accept as a
/// terminal tail or reject with its own diagnostic.
fn desugar_restart_loops(statements: &mut Vec<Stmt>) {
    let mut index = 0;
    while index < statements.len() {
        let Some(name) = marker_name(&statements[index], LABEL_MARKER_PREFIX).map(str::to_string)
        else {
            descend_restart_loops(&mut statements[index]);
            index += 1;
            continue;
        };
        let tail = &statements[index + 1..];
        // A jump from BEFORE the label is a forward jump into the tail, which this rewrite cannot
        // express — the `while` would have to be entered past its own top. Leave the whole label
        // to `collect_labels`, which accepts it when the tail is terminal and rejects it otherwise.
        if tail.is_empty()
            || !contains_goto(tail, &name)
            || contains_goto(&statements[..index], &name)
            || contains_label_marker(tail)
        {
            descend_restart_loops(&mut statements[index]);
            index += 1;
            continue;
        }
        let span = statements[index].span;
        let mut body: Vec<Stmt> = statements.split_off(index + 1);
        statements.pop(); // the label marker itself
        rewrite_restart_gotos(&mut body, &name, 0);
        body.push(Stmt::new(StmtKind::Break(1), span));
        statements.push(Stmt::new(
            StmtKind::While {
                condition: Expr::new(ExprKind::BoolLiteral(true), span),
                body,
            },
            span,
        ));
        // The tail is now the loop body; its own nested labels were excluded above, but the body
        // still has to be visited for labels sitting deeper inside it.
        let loop_index = statements.len() - 1;
        descend_restart_loops(&mut statements[loop_index]);
        index += 1;
    }
}

/// Turns forward jumps to labels of one statement list into guarded segments of that list.
///
/// Symfony's DeepClone polyfill ends its per-value loop body with two labels that earlier branches
/// jump to — `goto prepare_value;` skips the property walk, `goto handle_value;` skips everything
/// up to the bookkeeping — and neither tail returns: the body continues with the next element.
/// Cloning a tail cannot express that, but splitting the list at its labels can:
///
/// ```php
/// $sel = 0;
/// do { P0 } while (false);                // `goto Li` → `$sel = i; break <n>;`
/// if ($sel <= 1) { do { P1 } while (false); }
/// if ($sel <= 2) { P2 }
/// ```
///
/// Falling off a segment leaves the selector where it was, so the next guard admits it; a jump sets
/// the selector to its label and leaves the segment, so every guard before the label rejects it and
/// every one from the label on admits it. Only a segment that jumps is wrapped, and inside it the
/// `break N` / `continue N` that reach past the wrapper are renumbered to count it, exactly as
/// [`rewrite_restart_gotos`] does for its synthetic loop.
///
/// A label qualifies when every jump to it comes from EARLIER in its list, at any depth of this
/// function scope, and its tail does not already return or throw on its own (that shape keeps the
/// cloning strategy). Inner lists are rewritten first, so an outer jump that sits inside an inner
/// segment counts that segment's wrapper too.
fn desugar_forward_segments(statements: &mut Vec<Stmt>) {
    for statement in statements.iter_mut() {
        for_each_nested_body(statement, &mut desugar_forward_segments);
    }
    let mut targets: HashMap<String, usize> = HashMap::new();
    let mut cut_points: Vec<usize> = Vec::new();
    for (index, statement) in statements.iter().enumerate() {
        let Some(name) = marker_name(statement, LABEL_MARKER_PREFIX) else {
            continue;
        };
        let tail = &statements[index + 1..];
        let forward_only = contains_goto(&statements[..index], name) && !contains_goto(tail, name);
        let clonable_tail = statements_terminate(tail) && !contains_label_marker(tail);
        if forward_only && !clonable_tail {
            cut_points.push(index);
            targets.insert(name.to_string(), cut_points.len());
        }
    }
    if cut_points.is_empty() {
        return;
    }
    let span = statements[cut_points[0]].span;
    let selector = format!("__elephc_goto_{}_{}", span.line, span.col);
    let mut segments: Vec<Vec<Stmt>> = Vec::with_capacity(cut_points.len() + 1);
    let mut rest = std::mem::take(statements);
    for &cut in cut_points.iter().rev() {
        let mut tail = rest.split_off(cut);
        tail.remove(0); // the label marker itself
        segments.push(tail);
    }
    segments.push(rest);
    segments.reverse();

    let mut rewritten = vec![Stmt::new(
        StmtKind::Assign {
            name: selector.clone(),
            value: Expr::new(ExprKind::IntLiteral(0), span),
        },
        span,
    )];
    for (index, mut segment) in segments.into_iter().enumerate() {
        let jumps = targets.keys().any(|name| contains_goto(&segment, name));
        if jumps {
            rewrite_forward_gotos(&mut segment, &targets, &selector, 0);
            segment = vec![Stmt::new(
                StmtKind::DoWhile {
                    body: segment,
                    condition: Expr::new(ExprKind::BoolLiteral(false), span),
                },
                span,
            )];
        }
        if index == 0 {
            rewritten.extend(segment);
            continue;
        }
        let guard = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::Variable(selector.clone()), span)),
                op: BinOp::LtEq,
                right: Box::new(Expr::new(ExprKind::IntLiteral(index as i64), span)),
            },
            span,
        );
        rewritten.push(Stmt::new(
            StmtKind::If {
                condition: guard,
                then_body: segment,
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ));
    }
    *statements = rewritten;
}

/// Replaces each forward `goto` to one of `targets` with the selector store and the `break` that
/// leaves the segment wrapper, renumbering the levels that now have to count that wrapper.
///
/// `depth` is how many `break`-able constructs enclose `statements` within the segment.
fn rewrite_forward_gotos(
    statements: &mut [Stmt],
    targets: &HashMap<String, usize>,
    selector: &str,
    depth: usize,
) {
    for statement in statements.iter_mut() {
        if let Some(target) = marker_name(statement, GOTO_MARKER_PREFIX)
            .and_then(|name| targets.get(name))
            .copied()
        {
            let span = statement.span;
            statement.kind = StmtKind::Synthetic(vec![
                Stmt::new(
                    StmtKind::Assign {
                        name: selector.to_string(),
                        value: Expr::new(ExprKind::IntLiteral(target as i64), span),
                    },
                    span,
                ),
                Stmt::new(StmtKind::Break(depth + 1), span),
            ]);
            continue;
        }
        match &mut statement.kind {
            StmtKind::Break(levels) if *levels > depth => *levels += 1,
            StmtKind::Continue(levels) if *levels > depth => *levels += 1,
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                rewrite_forward_gotos(body, targets, selector, depth + 1)
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    rewrite_forward_gotos(body, targets, selector, depth + 1);
                }
                if let Some(body) = default {
                    rewrite_forward_gotos(body, targets, selector, depth + 1);
                }
            }
            _ => for_each_nested_body(statement, &mut |body| {
                rewrite_forward_gotos(body, targets, selector, depth)
            }),
        }
    }
}

/// Applies [`desugar_restart_loops`] to every statement list nested in one statement, staying
/// inside the current function scope exactly as [`collect_nested_labels`] does.
fn descend_restart_loops(statement: &mut Stmt) {
    for_each_nested_body(statement, &mut desugar_restart_loops);
}

/// Replaces each `goto <name>` in `statements` with the `continue` that reaches the synthetic
/// loop wrapping the tail, and renumbers the `break`/`continue` levels that now have to count it.
///
/// `depth` is how many `break`-able constructs enclose `statements` within the tail, so the
/// synthetic loop is level `depth + 1` from here.
fn rewrite_restart_gotos(statements: &mut [Stmt], name: &str, depth: usize) {
    for statement in statements.iter_mut() {
        if marker_name(statement, GOTO_MARKER_PREFIX) == Some(name) {
            statement.kind = StmtKind::Continue(depth + 1);
            continue;
        }
        match &mut statement.kind {
            // `levels > depth` reaches past the tail, so it targets a construct outside the
            // synthetic loop and has to count it; anything at or below `depth` stays put.
            StmtKind::Break(levels) if *levels > depth => *levels += 1,
            StmtKind::Continue(levels) if *levels > depth => *levels += 1,
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => rewrite_restart_gotos(body, name, depth + 1),
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    rewrite_restart_gotos(body, name, depth + 1);
                }
                if let Some(body) = default {
                    rewrite_restart_gotos(body, name, depth + 1);
                }
            }
            _ => for_each_nested_body(statement, &mut |body| {
                rewrite_restart_gotos(body, name, depth)
            }),
        }
    }
}

/// Returns whether a `goto <name>` marker appears anywhere in this scope's statement lists.
fn contains_goto(statements: &[Stmt], name: &str) -> bool {
    statements.iter().any(|statement| {
        marker_name(statement, GOTO_MARKER_PREFIX) == Some(name)
            || nested_bodies(statement)
                .iter()
                .any(|body| contains_goto(body, name))
    })
}

/// Returns whether any label marker appears anywhere in this scope's statement lists.
fn contains_label_marker(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| {
        marker_name(statement, LABEL_MARKER_PREFIX).is_some()
            || nested_bodies(statement)
                .iter()
                .any(|body| contains_label_marker(body))
    })
}

/// Borrows every statement list nested directly in `statement`, with the same scope boundary
/// [`for_each_nested_body`] observes.
fn nested_bodies(statement: &Stmt) -> Vec<&[Stmt]> {
    let mut bodies: Vec<&[Stmt]> = Vec::new();
    match &statement.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            bodies.push(then_body);
            bodies.extend(elseif_clauses.iter().map(|(_, body)| body.as_slice()));
            if let Some(body) = else_body {
                bodies.push(body);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            bodies.push(then_body);
            if let Some(body) = else_body {
                bodies.push(body);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => bodies.push(body),
        StmtKind::Switch { cases, default, .. } => {
            bodies.extend(cases.iter().map(|(_, body)| body.as_slice()));
            if let Some(body) = default {
                bodies.push(body);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            bodies.push(try_body);
            bodies.extend(catches.iter().map(|catch| catch.body.as_slice()));
            if let Some(body) = finally_body {
                bodies.push(body);
            }
        }
        _ => {}
    }
    bodies
}

/// Runs `visit` on every statement list nested directly in `statement`, without crossing into a
/// nested function, closure or class body — those are separate PHP scopes with their own labels.
fn for_each_nested_body(statement: &mut Stmt, visit: &mut dyn FnMut(&mut Vec<Stmt>)) {
    match &mut statement.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            visit(then_body);
            for (_, body) in elseif_clauses {
                visit(body);
            }
            if let Some(body) = else_body {
                visit(body);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            visit(then_body);
            if let Some(body) = else_body {
                visit(body);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => visit(body),
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                visit(body);
            }
            if let Some(body) = default {
                visit(body);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            visit(try_body);
            for catch in catches {
                visit(&mut catch.body);
            }
            if let Some(body) = finally_body {
                visit(body);
            }
        }
        _ => {}
    }
}

/// Creates a parser-only marker statement for a goto or label.
fn marker_stmt(prefix: &str, name: &str, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Synthetic(vec![Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::StringLiteral(format!("{}{}", prefix, name)),
                span,
            )),
            span,
        )]),
        span,
    )
}

/// Extracts a parser-only marker name when `statement` is a goto or label marker.
fn marker_name<'a>(statement: &'a Stmt, prefix: &str) -> Option<&'a str> {
    let StmtKind::Synthetic(body) = &statement.kind else {
        return None;
    };
    let [inner] = body.as_slice() else {
        return None;
    };
    let StmtKind::ExprStmt(expr) = &inner.kind else {
        return None;
    };
    let ExprKind::StringLiteral(value) = &expr.kind else {
        return None;
    };
    value.strip_prefix(prefix)
}

/// Collects each supported label and the terminal statement tail that begins after it.
///
/// `continuation` is what runs when this statement list falls off its end, for the block shapes
/// where that is statically the code after the enclosing statement — an `if` branch, essentially.
/// A label's tail is its own block's remainder, and when that alone does not terminate the
/// continuation is appended, because executing from the label really does run both.
/// `Symfony\Component\Routing\Matcher\Dumper\CompiledUrlMatcherTrait::match` needs exactly
/// that: `redirect_scheme:` opens an `elseif` branch that can fall through, and what it falls
/// through to is the `throw` after the whole chain.
fn collect_labels(
    statements: &[Stmt],
    continuation: &[Stmt],
    labels: &mut HashMap<String, Vec<Stmt>>,
) -> Result<(), CompileError> {
    for (index, statement) in statements.iter().enumerate() {
        if let Some(name) = marker_name(statement, LABEL_MARKER_PREFIX) {
            let mut tail = statements[index + 1..].to_vec();
            if !statements_terminate(&tail) {
                tail.extend_from_slice(continuation);
            }
            if tail.is_empty() || !statements_terminate(&tail) || contains_marker(&tail) {
                return Err(unsupported_goto_error(
                    statement.span,
                    name,
                    "its target does not begin a self-contained terminal tail",
                ));
            }
            if labels.insert(name.to_string(), tail).is_some() {
                return Err(CompileError::new(
                    statement.span,
                    &format!("Label '{}' has already been defined in this scope", name),
                ));
            }
        }
        let inner_continuation = block_continuation(statements, index, continuation);
        collect_nested_labels(statement, &inner_continuation, labels)?;
    }
    Ok(())
}

/// Returns what runs after the statement at `index` falls through, for its own nested blocks.
///
/// Only the statements that follow it in this list, plus this list's own continuation. Building it
/// here keeps `collect_nested_labels` a pure dispatch over the shapes that may USE it.
fn block_continuation(statements: &[Stmt], index: usize, continuation: &[Stmt]) -> Vec<Stmt> {
    let mut inner = statements[index + 1..].to_vec();
    inner.extend_from_slice(continuation);
    inner
}

/// Descends through structured statement bodies while staying inside the current function scope.
fn collect_nested_labels(
    statement: &Stmt,
    continuation: &[Stmt],
    labels: &mut HashMap<String, Vec<Stmt>>,
) -> Result<(), CompileError> {
    match &statement.kind {
        // A branch that falls off its end runs whatever follows the whole `if`, so its labels may
        // borrow the continuation.
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            collect_labels(then_body, continuation, labels)?;
            for (_, body) in elseif_clauses {
                collect_labels(body, continuation, labels)?;
            }
            if let Some(body) = else_body {
                collect_labels(body, continuation, labels)?;
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_labels(then_body, continuation, labels)?;
            if let Some(body) = else_body {
                collect_labels(body, continuation, labels)?;
            }
        }
        // A transparent block is its enclosing list, so it keeps the same continuation.
        StmtKind::IncludeOnceGuard { body, .. } | StmtKind::NamespaceBlock { body, .. } => {
            collect_labels(body, continuation, labels)?
        }
        // Falling off a LOOP body starts the next iteration and falling off a `switch` case leaves
        // the switch, so neither reaches the continuation; falling off a `try` runs `finally`
        // first. None of those may borrow it, so they collect with an empty one and a label there
        // still needs a tail that terminates on its own.
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => collect_labels(body, &[], labels)?,
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                collect_labels(body, &[], labels)?;
            }
            if let Some(body) = default {
                collect_labels(body, &[], labels)?;
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_labels(try_body, &[], labels)?;
            for catch in catches {
                collect_labels(&catch.body, &[], labels)?;
            }
            if let Some(body) = finally_body {
                collect_labels(body, &[], labels)?;
            }
        }
        StmtKind::Synthetic(body) if marker_name(statement, LABEL_MARKER_PREFIX).is_none() => {
            collect_labels(body, continuation, labels)?;
        }
        _ => {}
    }
    Ok(())
}

/// Replaces goto markers with their cloned terminal tails and removes label markers.
fn rewrite_gotos(
    statements: &mut Vec<Stmt>,
    labels: &HashMap<String, Vec<Stmt>>,
) -> Result<(), CompileError> {
    let mut rewritten = Vec::with_capacity(statements.len());
    for mut statement in std::mem::take(statements) {
        if marker_name(&statement, LABEL_MARKER_PREFIX).is_some() {
            continue;
        }
        if let Some(name) = marker_name(&statement, GOTO_MARKER_PREFIX) {
            let Some(tail) = labels.get(name) else {
                return Err(unsupported_goto_error(
                    statement.span,
                    name,
                    "no matching terminal label exists in this scope",
                ));
            };
            statement.kind = StmtKind::Synthetic(tail.clone());
            rewritten.push(statement);
            continue;
        }
        rewrite_nested_gotos(&mut statement, labels)?;
        rewritten.push(statement);
    }
    *statements = rewritten;
    Ok(())
}

/// Descends through structured statement bodies to rewrite gotos in the current scope.
fn rewrite_nested_gotos(
    statement: &mut Stmt,
    labels: &HashMap<String, Vec<Stmt>>,
) -> Result<(), CompileError> {
    match &mut statement.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            rewrite_gotos(then_body, labels)?;
            for (_, body) in elseif_clauses {
                rewrite_gotos(body, labels)?;
            }
            if let Some(body) = else_body {
                rewrite_gotos(body, labels)?;
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            rewrite_gotos(then_body, labels)?;
            if let Some(body) = else_body {
                rewrite_gotos(body, labels)?;
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => rewrite_gotos(body, labels)?,
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                rewrite_gotos(body, labels)?;
            }
            if let Some(body) = default {
                rewrite_gotos(body, labels)?;
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            rewrite_gotos(try_body, labels)?;
            for catch in catches {
                rewrite_gotos(&mut catch.body, labels)?;
            }
            if let Some(body) = finally_body {
                rewrite_gotos(body, labels)?;
            }
        }
        StmtKind::Synthetic(body) => rewrite_gotos(body, labels)?,
        _ => {}
    }
    Ok(())
}

/// Returns whether a statement list cannot fall through its final statement.
fn statements_terminate(statements: &[Stmt]) -> bool {
    statements.last().is_some_and(statement_terminates)
}

/// Returns whether one structured statement guarantees a return or throw on every path.
fn statement_terminates(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Return(_) | StmtKind::Throw(_) => true,
        StmtKind::Synthetic(body) => statements_terminate(body),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body: Some(else_body),
            ..
        } => {
            statements_terminate(then_body)
                && elseif_clauses
                    .iter()
                    .all(|(_, body)| statements_terminate(body))
                && statements_terminate(else_body)
        }
        StmtKind::IfDef {
            then_body,
            else_body: Some(else_body),
            ..
        } => statements_terminate(then_body) && statements_terminate(else_body),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            finally_body.as_ref().is_some_and(|body| statements_terminate(body))
                || (statements_terminate(try_body)
                    && !catches.is_empty()
                    && catches.iter().all(|catch| statements_terminate(&catch.body)))
        }
        _ => false,
    }
}

/// Returns whether a prospective terminal tail still contains a goto or label marker.
fn contains_marker(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| {
        marker_name(statement, GOTO_MARKER_PREFIX).is_some()
            || marker_name(statement, LABEL_MARKER_PREFIX).is_some()
    })
}

/// Builds the diagnostic for a goto outside the deliberately supported terminal-tail subset.
fn unsupported_goto_error(span: Span, label: &str, reason: &str) -> CompileError {
    CompileError::new(
        span,
        &format!(
            "`goto` is not supported for label `{}`: {}; elephc currently supports only jumps to a label whose remaining block always returns or throws",
            label, reason
        ),
    )
}
