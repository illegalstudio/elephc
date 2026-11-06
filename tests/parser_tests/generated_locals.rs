//! Purpose:
//! Pins that every local the PARSER synthesizes carries the shared generated-local marker.
//!
//! Called from:
//! - `tests/parser_tests.rs` through Rust's test harness.
//!
//! Key details:
//! - Source-only: the snippets are parsed and inspected, never compiled or executed.
//! - The assertion reads the parsed AST's own Debug rendering, so it sees every synthesized
//!   name whatever AST node holds it, without a hand-written walker that a newly added node
//!   could silently fall out of.
//! - `elephc::names::is_generated_local_name` is the ONE predicate `get_defined_vars()` and eval
//!   scope synchronization filter on, so a parser temporary that fails it is PHP-visible.

use elephc::names::is_generated_local_name;

use super::*;

/// Intermediate and final probes bracket compound reads without reading plain-write children.
#[test]
fn append_live_guard_roles_bracket_dimension_reads() {
    for (operator, expected) in [
        ("=", vec!["parent_pre", "final_pre"]),
        ("+=", vec!["parent_pre", "read", "parent_post", "final_pre", "read", "final_post"]),
    ] {
        let statements = parse_source(&format!("<?php echo ($items[][1.5][2.5] {operator} 7);"));
        let StmtKind::Echo(expression) = &statements[0].kind else { panic!("expected echo"); };
        let ExprKind::Assignment { prelude, .. } = &expression.kind else { panic!("expected assignment"); };
        let order: Vec<_> = prelude.iter().filter_map(|stmt| {
            let StmtKind::Assign { name, value } = &stmt.kind else { return None; };
            if name.starts_with("__elephc_append_live_guard_") {
                assert!(is_generated_local_name(name));
                ["parent_pre", "parent_post", "final_pre", "final_post"].into_iter()
                    .find(|role| name.ends_with(&format!("_{role}#gen")))
            } else if matches!(value.kind, ExprKind::ArrayAccess { .. }) { Some("read") }
            else { None }
        }).collect();
        assert_eq!(order, expected, "{operator}");
    }
}

/// Plain converted-key appends publish their parent before an unread unforgeable live guard.
#[test]
fn append_plain_live_guard_preserves_rhs_publication_order() {
    let statements = parse_source("<?php echo ($items[][1.5] = rhs());");
    let StmtKind::Echo(expression) = &statements[0].kind else { panic!("expected echo"); };
    let ExprKind::Assignment { prelude, .. } = &expression.kind else { panic!("expected assignment"); };
    let rhs = prelude.iter().position(|stmt| matches!(&stmt.kind,
        StmtKind::Assign { value, .. } if matches!(&value.kind,
            ExprKind::FunctionCall { name, .. } if name.as_str() == "rhs"))).unwrap();
    let push = prelude.iter().position(|stmt| matches!(&stmt.kind,
        StmtKind::ArrayPush { array, value } if array == "items" && matches!(value.kind, ExprKind::Null))).unwrap();
    let guard = prelude.iter().position(|stmt| matches!(&stmt.kind,
        StmtKind::Assign { name, .. } if name.starts_with("__elephc_append_live_guard_"))).unwrap();
    assert!(rhs < push && push < guard);
    let StmtKind::Assign { name, value } = &prelude[guard].kind else { unreachable!() };
    assert!(is_generated_local_name(name));
    assert!(matches!(&value.kind, ExprKind::Variable(name)
        if name.starts_with("__elephc_append_update_cell_") && is_generated_local_name(name)));
    assert!(!prelude.iter().any(|stmt| matches!(&stmt.kind,
        StmtKind::Assign { value, .. } if matches!(value.kind, ExprKind::ArrayAccess { .. }))),
        "a plain write must not read a missing child to trigger its live guard");
}

/// Returns every name in the parsed AST that looks like a compiler-synthesized local.
///
/// The candidate set is deliberately wider than the marker itself: it also catches a name that
/// still carries only the legacy `__elephc` spelling, which is exactly the migration failure
/// this module exists to detect. Names are read out of the Debug rendering by splitting on the
/// quote character, which is sound because no name this parser mints can contain one.
fn synthesized_name_candidates(source: &str) -> Vec<String> {
    let rendered = format!("{:?}", parse_source(source));
    rendered
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|segment| segment.contains("__elephc") || segment.contains('#'))
        .map(str::to_string)
        .collect()
}

/// Asserts that `source` synthesizes at least one local whose stem is `stem`, and that EVERY
/// synthesized local it produces is recognised by the shared predicate.
fn assert_marked_local_with_stem(source: &str, stem: &str) {
    let candidates = synthesized_name_candidates(source);
    for name in &candidates {
        assert!(
            is_generated_local_name(name),
            "parser local {name} is not recognised as generated, so it stays PHP-visible"
        );
    }
    assert!(
        candidates.iter().any(|name| name.starts_with(stem)),
        "expected a {stem}* temporary from this snippet, saw {candidates:?}"
    );
}

/// A `foreach` destructuring pattern binds its element through a marked local.
#[test]
fn foreach_destructuring_value_local_is_marked() {
    assert_marked_local_with_stem(
        "<?php foreach ($m as [$a, $b]) { echo $a; }",
        "__elephc_foreach_",
    );
}

/// A postfix increment on an array element in expression position captures the old value in
/// one marked local and stores the new value in another.
///
/// Statement position never reaches this desugaring, so the snippet has to consume the value.
#[test]
fn postfix_incdec_expression_locals_are_marked() {
    let source = "<?php $x = $items[0]++;";
    assert_marked_local_with_stem(source, "__elephc_incdec_old_");
    let candidates = synthesized_name_candidates(source);
    assert!(
        candidates
            .iter()
            .any(|name| name.starts_with("__elephc_incdec_new_")),
        "expected the new-value temporary too, saw {candidates:?}"
    );
}

/// A compound assignment used as an EXPRESSION binds its result through a marked local.
///
/// `bind_result_value` always mints a temporary there, because the result is referenced twice.
#[test]
fn assignment_expression_temp_is_marked() {
    assert_marked_local_with_stem("<?php $x = ($items[0] .= 'y');", "__elephc_assign_expr_");
}

/// A destructuring assignment that is not a flat run of bare variables binds its source through
/// a marked local.
///
/// A flat positional pattern takes the simpler `ListUnpack` path and mints nothing, so the
/// snippet uses keyed entries to reach the lowerer.
#[test]
fn list_destructuring_temp_is_marked() {
    assert_marked_local_with_stem("<?php ['x' => $a, 'y' => $b] = build();", "__elephc_list_");
}

/// A compound assignment through an index with side effects settles it into a marked local.
#[test]
fn compound_assignment_temp_is_marked() {
    assert_marked_local_with_stem("<?php $items[idx()] += 3;", "__elephc_compound_");
}

/// A nested append keeps its own reserved prefix AND the marker.
///
/// The prefix is what IR lowering matches on to fuse the append, so it has to survive the
/// migration; the marker is what keeps the temporary out of PHP-visible scope. Both at once.
#[test]
fn nested_append_temp_keeps_its_prefix_and_the_marker() {
    assert_marked_local_with_stem("<?php $groups[0][] = 1;", "__elephc_napp_");
}

/// A user variable merely SPELLED like a parser temporary stays PHP-visible.
///
/// This is the other half of the migration: the marker is unforgeable from source, so the
/// predicate can no longer be fooled by an `__elephc`-prefixed name a program is entitled to
/// write.
#[test]
fn a_user_variable_spelled_like_a_parser_temp_is_not_generated() {
    assert!(!is_generated_local_name("__elephc_list_1_1_0"));
    assert!(!is_generated_local_name("__elephc_napp_1_1_0"));
    let rendered = format!("{:?}", parse_source("<?php $__elephc_foreach_1_1 = 1;"));
    assert!(rendered.contains("__elephc_foreach_1_1"));
    assert!(!rendered.contains("__elephc_foreach_1_1#gen"));
}
