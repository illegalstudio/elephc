//! Purpose:
//! Unit tests for `goto` lowering: the shape of the structured code it produces and the
//! diagnostics it reports for invalid jumps.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Shapes are asserted on the AST. Run-time behaviour against reference PHP is covered by the
//!   end-to-end tests in `tests/codegen/control_flow/goto.rs`.

use super::*;
use crate::parser::ast::StmtKind;

/// Parses `source` and lowers its jumps the way per-file finalization does.
fn lower(source: &str) -> Result<Program, CompileError> {
    let program = crate::parser::parse_test_source(source).expect("fixture must parse");
    lower_program(program, "fixture.php")
}

/// Returns the message of the first diagnostic lowering `source` reports.
fn error(source: &str) -> String {
    let error = lower(source).expect_err("fixture must be refused");
    error.message
}

/// Returns whether any statement of `stmts`, at any depth of this scope, satisfies `test`.
fn any_stmt(stmts: &[Stmt], test: &dyn Fn(&StmtKind) -> bool) -> bool {
    stmts.iter().any(|stmt| {
        test(&stmt.kind)
            || plan::child_blocks(&stmt.kind)
                .into_iter()
                .any(|(_, body)| any_stmt(body, test))
            || matches!(&stmt.kind, StmtKind::FunctionDecl { body, .. } if any_stmt(body, test))
    })
}

/// Returns the first statement of the first `catch` body found in `stmts`, at any depth.
fn first_stmt_of_first_catch(stmts: &[Stmt]) -> Option<&StmtKind> {
    stmts.iter().find_map(|stmt| {
        if let StmtKind::Try { catches, .. } = &stmt.kind {
            if let Some(clause) = catches.first() {
                return clause.body.first().map(|first| &first.kind);
            }
        }
        plan::child_blocks(&stmt.kind)
            .into_iter()
            .find_map(|(_, body)| first_stmt_of_first_catch(body))
    })
}

/// Asserts no jump or label survives lowering anywhere in `program`.
fn assert_fully_lowered(program: &[Stmt]) {
    assert!(!any_stmt(program, &|kind| matches!(
        kind,
        StmtKind::Goto(_) | StmtKind::GotoLabel(_)
    )));
}

/// Verifies a label no `goto` targets is dropped, in a file with no `goto` at all too.
#[test]
fn removes_labels_without_a_goto() {
    let program = lower("<?php unused: echo 1; function f() { other: return 1; }").expect("lowers");
    assert_fully_lowered(&program);
    assert!(matches!(program[0].kind, StmtKind::Echo(_)));
}

/// Verifies PHP's duplicate-label error does not depend on a `goto` being present.
#[test]
fn reports_duplicate_labels_without_a_goto() {
    assert_eq!(error("<?php a: echo 1; a: echo 2;"), "Label 'a' already defined");
    assert_eq!(
        error("<?php function f() { a: echo 1; a: echo 2; }"),
        "Label 'a' already defined"
    );
    assert_eq!(
        error("<?php $f = function () { a: echo 1; a: echo 2; };"),
        "Label 'a' already defined"
    );
}

/// Verifies the braced `namespace` blocks of a file share its label scope: a name may not
/// repeat across them, a jump inside one block is lowered, and a jump from one block to
/// another finds its label and is refused as a namespace crossing.
#[test]
fn braced_namespace_blocks_share_the_file_scope() {
    assert_eq!(
        error("<?php namespace A { l: echo 1; } namespace B { l: echo 2; }"),
        "Label 'l' already defined"
    );
    assert!(error("<?php namespace A { goto l; } namespace B { l: echo 2; }")
        .contains("across a namespace declaration"));

    let program = lower(
        "<?php namespace A { $i = 0; again: $i++; if ($i < 3) goto again; } namespace B { echo 1; }",
    )
    .expect("lowers");
    assert_fully_lowered(&program);
    let StmtKind::NamespaceBlock { body, .. } = &program[0].kind else {
        panic!("the namespace block stays a top-level statement");
    };
    assert!(body.iter().any(|stmt| matches!(stmt.kind, StmtKind::While { .. })));
}

/// Verifies a backward jump to a leading label becomes a bare loop with `continue`.
#[test]
fn backward_jump_to_first_label_is_a_single_loop() {
    let program = lower("<?php $i = 0; again: $i++; if ($i < 3) goto again; echo $i;")
        .expect("lowers");
    assert_fully_lowered(&program);
    // `$i = 0;` stays in front, `echo $i;` stays behind, and no state local is needed.
    assert_eq!(program.len(), 3);
    let StmtKind::While { body, .. } = &program[1].kind else {
        panic!("expected the dispatch loop, got {:?}", program[1].kind);
    };
    assert!(!any_stmt(body, &|kind| matches!(kind, StmtKind::Assign { name, .. } if name.contains("__elephc_goto"))));
    assert!(any_stmt(body, &|kind| matches!(kind, StmtKind::Continue(1))));
    assert!(matches!(body.last().expect("loop body").kind, StmtKind::Break(1)));
}

/// Verifies a forward jump guards the skipped segment and leaves the last one unguarded.
#[test]
fn forward_jump_guards_the_skipped_segment() {
    let program = lower("<?php if ($argc > 5) goto end; echo 'body'; end: echo 'tail';")
        .expect("lowers");
    assert_fully_lowered(&program);
    // State initialization, the dispatch loop, then the statement after the label.
    assert_eq!(program.len(), 3);
    assert!(matches!(program[0].kind, StmtKind::Assign { .. }));
    let StmtKind::While { body, .. } = &program[1].kind else {
        panic!("expected the dispatch loop, got {:?}", program[1].kind);
    };
    // `if ($state === 0) { if (…) { $state = 1; continue; } echo 'body'; }`
    let StmtKind::If { then_body, else_body, .. } = &body[0].kind else {
        panic!("expected the guarded first segment, got {:?}", body[0].kind);
    };
    assert!(else_body.is_none());
    assert!(any_stmt(then_body, &|kind| matches!(kind, StmtKind::Continue(1))));
    // The label's segment: the state reset, then the exit from inside the segment.
    assert!(matches!(body[1].kind, StmtKind::Assign { .. }));
    assert!(matches!(body.last().expect("loop body").kind, StmtKind::Break(1)));
}

/// Verifies a wrapped range that cannot fall through gets no exit appended, so termination
/// analysis still sees a function that returns on every path.
#[test]
fn range_that_cannot_fall_through_gets_no_break() {
    let program = lower(
        "<?php function f(int $n): int { again: $n++; if ($n > 9) return $n; goto again; }",
    )
    .expect("lowers");
    let StmtKind::FunctionDecl { body, .. } = &program[0].kind else {
        panic!("expected the function, got {:?}", program[0].kind);
    };
    assert_eq!(body.len(), 1);
    let StmtKind::While { body, .. } = &body[0].kind else {
        panic!("expected the dispatch loop, got {:?}", body[0].kind);
    };
    assert!(matches!(body.last().expect("loop body").kind, StmtKind::Continue(1)));
    assert!(!any_stmt(body, &|kind| matches!(kind, StmtKind::Break(_))));
}

/// Verifies `break` and `continue` written inside a wrapped loop body keep their targets.
#[test]
fn renumbers_breaks_that_cross_the_dispatch_loop() {
    let program = lower(
        "<?php while (true) { again: if ($argc > 1) break; if ($argc > 2) continue; goto again; }",
    )
    .expect("lowers");
    assert_fully_lowered(&program);
    // The user's `break` and `continue` now sit one level deeper, under the dispatch loop.
    assert!(any_stmt(&program, &|kind| matches!(kind, StmtKind::Break(2))));
    assert!(any_stmt(&program, &|kind| matches!(kind, StmtKind::Continue(2))));
    // The jump itself restarts the dispatch loop.
    assert!(any_stmt(&program, &|kind| matches!(kind, StmtKind::Continue(1))));
}

/// Verifies an out-of-range `break` inside a dispatch loop is reported with the level the
/// program wrote.
#[test]
fn reports_out_of_range_breaks_with_the_written_level() {
    assert_eq!(error("<?php again: echo 1; break; goto again;"), "Cannot 'break' 1 levels");
    assert_eq!(
        error("<?php while ($argc > 9) { again: continue 3; goto again; }"),
        "Cannot 'continue' 3 levels"
    );
}

/// Verifies a `catch` body starts by zeroing the jump state when the scope's jumps use it, and
/// is left as written when they are bare `continue`s.
#[test]
fn catch_bodies_clear_a_cancelled_jump() {
    let stateful = lower(
        "<?php goto b; a: echo 1; b: try { goto a; } catch (Exception $e) { echo 2; }",
    )
    .expect("lowers");
    assert!(matches!(first_stmt_of_first_catch(&stateful), Some(StmtKind::Assign { .. })));

    let stateless =
        lower("<?php a: try { goto a; } catch (Exception $e) { echo 2; }").expect("lowers");
    assert!(matches!(first_stmt_of_first_catch(&stateless), Some(StmtKind::Echo(_))));
}

/// Verifies a jump into an `if` branch guards the condition with the pending label.
#[test]
fn jump_into_if_branch_guards_the_condition() {
    let program = lower("<?php goto in; if ($argc > 5) { echo 'a'; in: echo 'b'; } echo 'c';")
        .expect("lowers");
    assert_fully_lowered(&program);
    let found = any_stmt(&program, &|kind| match kind {
        StmtKind::If { condition, then_body, .. } => {
            matches!(&condition.kind, crate::parser::ast::ExprKind::BinaryOp { op, .. }
                if *op == crate::parser::ast::BinOp::Or)
                && then_body.iter().any(|stmt| matches!(stmt.kind, StmtKind::While { .. }))
        }
        _ => false,
    });
    assert!(found, "the entered branch is guarded and holds its own dispatch loop");
}

/// Verifies a function body is lowered as its own label scope.
#[test]
fn lowers_function_bodies_as_separate_scopes() {
    let program = lower("<?php function f() { a: goto a; } a: goto a;").expect("lowers");
    assert_fully_lowered(&program);
}

/// Verifies PHP's wording for a jump to a label that does not exist in the scope.
#[test]
fn reports_undefined_labels() {
    assert_eq!(error("<?php goto nope;"), "'goto' to undefined label 'nope'");
    assert_eq!(
        error("<?php function f() { goto x; } x: echo 1;"),
        "'goto' to undefined label 'x'"
    );
    assert_eq!(
        error("<?php $f = function () { goto x; }; x: echo 1;"),
        "'goto' to undefined label 'x'"
    );
}

/// Verifies label names are case-sensitive, as in PHP.
#[test]
fn label_names_are_case_sensitive() {
    assert_eq!(error("<?php goto A; a: echo 1;"), "'goto' to undefined label 'A'");
}

/// Verifies PHP's wording for a label declared twice in one scope.
#[test]
fn reports_duplicate_labels() {
    assert_eq!(error("<?php a: echo 1; a: goto a;"), "Label 'a' already defined");
}

/// Verifies PHP's wording for a jump into a loop or a `switch` from outside it.
#[test]
fn reports_jumps_into_loops_and_switches() {
    let message = "'goto' into loop or switch statement is disallowed";
    assert_eq!(error("<?php goto in; while (true) { in: break; }"), message);
    assert_eq!(error("<?php goto in; foreach ([1] as $v) { in: echo $v; }"), message);
    assert_eq!(error("<?php goto in; switch (1) { case 1: in: echo 1; }"), message);
}

/// Verifies PHP's wording for jumps into and out of a `finally` block.
#[test]
fn reports_jumps_across_finally() {
    assert_eq!(
        error("<?php goto f; try { echo 1; } finally { f: echo 2; }"),
        "jump into a finally block is disallowed"
    );
    assert_eq!(
        error("<?php try { echo 1; } finally { goto o; } o: echo 3;"),
        "jump out of a finally block is disallowed"
    );
}

/// Verifies the shapes PHP accepts and this compiler refuses are named in the diagnostic.
#[test]
fn reports_unsupported_jump_shapes() {
    assert!(error("<?php goto c; try { echo 1; } catch (Exception $e) { c: echo 2; }")
        .contains("into a catch block"));
    assert!(error("<?php switch (1) { case 1: goto two; case 2: two: echo 2; }")
        .contains("another case"));
    assert!(error("<?php goto end; const X = 1; end: echo 1;").contains("across a const"));
    assert!(error("<?php again: const X = 1; if ($argc > 9) goto again;").contains("across a const"));
}

/// Verifies jumps whose statement spans do not touch get loops of their own, so a declaration
/// written between them stays in place, in order, at the top level.
#[test]
fn independent_jumps_get_separate_loops() {
    let program = lower(
        "<?php a: echo 1; if ($argc > 9) goto a; const X = 1; b: echo X; if ($argc > 8) goto b;",
    )
    .expect("lowers");
    assert_fully_lowered(&program);
    assert!(matches!(program[0].kind, StmtKind::While { .. }));
    assert!(matches!(program[1].kind, StmtKind::ConstDecl { .. }));
    assert!(matches!(program[2].kind, StmtKind::While { .. }));
    assert_eq!(program.len(), 3);
}

/// Verifies a `namespace` or `use` declaration between two independent jumps is accepted and
/// left between their loops, and one that a jump skips or repeats is refused.
#[test]
fn namespace_and_use_declarations_split_or_refuse() {
    let program = lower(
        "<?php namespace A; goto a; a: echo 1; namespace B; use A\\C; goto b; b: echo 2;",
    )
    .expect("lowers");
    assert_fully_lowered(&program);
    let shape: Vec<&str> = program
        .iter()
        .filter_map(|stmt| match stmt.kind {
            StmtKind::While { .. } => Some("loop"),
            StmtKind::NamespaceDecl { .. } => Some("namespace"),
            StmtKind::UseDecl { .. } => Some("use"),
            _ => None,
        })
        .collect();
    assert_eq!(shape, ["namespace", "loop", "namespace", "use", "loop"]);

    assert!(error("<?php namespace A; goto x; namespace B; x: echo 1;")
        .contains("across a namespace declaration"));
    assert!(error("<?php goto x; use A\\C; x: echo 1;").contains("across a use declaration"));
}

/// Verifies every invalid jump of a file is reported, not only the first.
#[test]
fn reports_every_invalid_jump() {
    let error = lower("<?php goto a; function f() { goto b; }").expect_err("refused");
    assert_eq!(error.related.len() + 1, 2);
}
