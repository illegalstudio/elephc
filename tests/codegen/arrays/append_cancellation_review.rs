//! Purpose:
//! Regressions for append selection after diagnostic callbacks separate a container.
//!
//! Called from:
//! - The array codegen integration suite.
//!
//! Key details:
//! - Cancellation follows the selected nested container, not unrelated root mutations.
//! - Retaining a by-value copy can preserve the old selected container for the update.
//! - Abandoned intermediate paths still finish; final selected-cell loss cancels the operation.

use crate::support::*;

/// Checks diagnostics and ownership when the first key callback abandons an intermediate place.
fn verify_intermediate_abandonment(callback: &str, root: &str, dimensions: &str, operation: &str) {
    let source = format!(r#"<?php
function rhs(): int {{ echo 'R|'; return 7; }}
function run(array $items): void {{
    $left = []; $right = []; $seen = 0;
    set_error_handler(function(int $level, string $message) use (&$items, &$left, &$right, &$seen): bool {{
        ++$seen; echo $level, ':', json_encode($items), '|';
        if ($seen === 1) {{ {callback} }} return true;
    }});
    $result = ($items[][{dimensions}] {operation} rhs());
    restore_error_handler();
    echo json_encode($result), ':', json_encode($items), ':', json_encode($left), ':', json_encode($right);
    unset($items, $left, $right, $result);
}}
run([]);
"#);
    let later_key = if dimensions.contains("2.5") { format!("8192:{root}|") } else { String::new() };
    let missing_leaf = if operation == "+=" { format!("2:{root}|") } else { String::new() };
    assert_output(&source, &format!("R|8192:[[]]|{later_key}{missing_leaf}7:{root}:[]:[]"));
}

/// A lost intermediate parent continues later key diagnostics and returns the normal value.
#[test]
fn test_append_grok_current_intermediate_abandonment() {
    for operation in ["=", "+="] {
        verify_intermediate_abandonment("$items = ['gone' => true];", "{\"gone\":true}", "1.5][2.5", operation);
        verify_intermediate_abandonment(
            "$left = $items[0]; $right = $items[0]; $items[0] = ['other' => 2];",
            "[{\"other\":2}]", "1.5][2.5", operation,
        );
    }
}

/// Earlier cancellation still returns the ordinary value when the final key is a literal.
#[test]
fn test_append_grok_current_intermediate_then_literal() {
    for operation in ["=", "+="] {
        verify_intermediate_abandonment("$items = ['gone' => true];", "{\"gone\":true}", "1.5][2", operation);
    }
}

/// Losing the final place during conversion cancels before any missing-leaf diagnostic.
#[test]
fn test_append_grok_current_final_key_abandonment() {
    for operation in ["=", "+="] {
        let source = format!(r#"<?php
function rhs(): int {{ echo 'R|'; return 7; }}
$items = [];
set_error_handler(function(int $level, string $message) use (&$items): bool {{
    echo $level, ':', json_encode($items), '|';
    if (str_contains($message, '2.5')) {{ $items = ['gone' => true]; }} return true;
}});
$result = ($items[][1.5][2.5] {operation} rhs());
restore_error_handler(); echo json_encode($result), ':', json_encode($items);
unset($items, $result);
"#);
        let intermediate_read = if operation == "+=" { "2:[[]]|" } else { "" };
        assert_output(&source, &format!(
            "R|8192:[[]]|{intermediate_read}8192:[{{\"1\":[]}}]|null:{{\"gone\":true}}",
        ));
    }
}

/// Abandoned intermediate selections do not depend on EIR optimization.
#[test]
fn test_append_grok_current_intermediate_abandonment_no_ir_opt() {
    without_ir_opt(test_append_grok_current_intermediate_abandonment);
}

/// A literal final key does not change unoptimized abandonment semantics.
#[test]
fn test_append_grok_current_intermediate_then_literal_no_ir_opt() {
    without_ir_opt(test_append_grok_current_intermediate_then_literal);
}

/// Final conversion cancellation skips stale reads without EIR optimization too.
#[test]
fn test_append_grok_current_final_key_abandonment_no_ir_opt() {
    without_ir_opt(test_append_grok_current_final_key_abandonment);
}

/// A missing intermediate read may detach, and a later throwing callback unwinds the scratch.
#[test]
fn test_append_grok_current_read_abandonment_and_throw() {
    for throwing in [false, true] {
        let source = r#"<?php
function rhs(): int { echo 'R|'; return 7; }
function run(array $items): void {
    $seen = 0;
    set_error_handler(function(int $level, string $message) use (&$items, &$seen): bool {
        ++$seen; echo $level, ':', json_encode($items), '|';
        if ($seen === 1) { $items = ['gone' => true]; }
        else { THROW_ACTION } return true;
    });
    try { $result = ($items[]['first'][2.5] += rhs()); echo json_encode($result), ':'; }
    catch (Error $error) { echo 'C:'; unset($error); }
    restore_error_handler(); echo json_encode($items); unset($items, $result);
}
run([]);
"#.replace("THROW_ACTION", if throwing { "throw new Error('stop');" } else { "" });
        assert_output(&source, if throwing {
            "R|2:[[]]|8192:{\"gone\":true}|C:{\"gone\":true}"
        } else { "R|2:[[]]|8192:{\"gone\":true}|2:{\"gone\":true}|7:{\"gone\":true}" });
    }
}

/// Intermediate-read loss and subsequent exceptions also retire unoptimized scratch owners.
#[test]
fn test_append_grok_current_read_abandonment_and_throw_no_ir_opt() {
    without_ir_opt(test_append_grok_current_read_abandonment_and_throw);
}

/// Live dimensions still interleave key conversion, missing reads and parent publication.
#[test]
fn test_append_grok_current_live_dimension_order() {
    assert_output(r#"<?php
function rhs(): int { echo 'R|'; return 7; }
$items = [];
set_error_handler(function(int $level, string $message) use (&$items): bool {
    echo $level, ':', json_encode($items), '|'; return true;
});
$result = ($items[][1.5][2.5][3.5] += rhs());
restore_error_handler(); echo json_encode($result), ':', json_encode($items);
unset($items, $result);
"#, "R|8192:[[]]|2:[[]]|8192:[{\"1\":[]}]|2:[{\"1\":[]}]|8192:[{\"1\":{\"2\":[]}}]|2:[{\"1\":{\"2\":[]}}]|7:[{\"1\":{\"2\":{\"3\":7}}}]");
}

/// Normal diagnostic order does not depend on EIR optimization.
#[test]
fn test_append_grok_current_live_dimension_order_no_ir_opt() {
    without_ir_opt(test_append_grok_current_live_dimension_order);
}

/// Plain nested append publishes its parent after the RHS but before key diagnostics.
#[test]
fn test_append_greptile_plain_parent_publication() {
    for throwing in [false, true] {
        let source = r#"<?php
function rhs(): int { echo 'R|'; return 1; }
$items = [];
set_error_handler(function(int $level, string $message) use (&$items): bool {
    echo 'H:', json_encode($items), '|'; HANDLER_ACTION
});
try { $items[][1.5] = rhs(); }
catch (Error $error) { echo 'C|'; unset($error); }
restore_error_handler(); echo json_encode($items); unset($items);
"#.replace("HANDLER_ACTION", if throwing { "throw new Error('stop');" } else { "return true;" });
        assert_output(&source, if throwing { "R|H:[[]]|C|[[]]" } else { "R|H:[[]]|[{\"1\":1}]" });
    }
}

/// Plain-parent publication is independent of EIR optimization.
#[test]
fn test_append_greptile_plain_parent_publication_no_ir_opt() {
    without_ir_opt(test_append_greptile_plain_parent_publication);
}

/// Plain writes follow the selected parent's surviving copy or cancel after separation.
#[test]
fn test_append_greptile_plain_selected_parent_cancellation() {
    for (callback, expected) in [
        ("$copy = $items[0]; $items[0] = ['other' => 2];", "1:[{\"other\":2}]:{\"1\":1}"),
        ("$items[0]['other'] = 2;", "null:[{\"other\":2}]:[]"),
        ("$items = [];", "null:[]:[]"),
    ] {
        let source = format!(r#"<?php
function run(array $items): void {{
    $copy = [];
    set_error_handler(function(int $level, string $message) use (&$items, &$copy): bool {{
        {callback} return true;
    }});
    $result = ($items[][1.5] = 1);
    restore_error_handler();
    echo json_encode($result), ':', json_encode($items), ':', json_encode($copy);
    unset($items, $copy, $result);
}}
run([]);
"#);
        assert_output(&source, expected);
    }
}

/// Plain selected-parent cancellation keeps its CFG without EIR optimization.
#[test]
fn test_append_greptile_plain_selected_parent_cancellation_no_ir_opt() {
    without_ir_opt(test_append_greptile_plain_selected_parent_cancellation);
}

/// Nested converted keys publish every intermediate before diagnosing that key exactly once.
#[test]
fn test_append_greptile_plain_multilevel_key_order() {
    assert_output(r#"<?php
function selectedKey(): float { echo 'K|'; return 1.5; }
function rhs(): int { echo 'R|'; return 7; }
$items = [];
set_error_handler(function(int $level, string $message) use (&$items): bool {
    echo 'H:', json_encode($items), '|'; return true;
});
$result = ($items[][selectedKey()][2.5] = rhs());
restore_error_handler(); echo json_encode($result), ':', json_encode($items);
unset($items, $result);
"#, "K|R|H:[[]]|H:[{\"1\":[]}]|7:[{\"1\":{\"2\":7}}]");
}

/// Multiple private live guards survive the unoptimized EIR path too.
#[test]
fn test_append_greptile_plain_multilevel_key_order_no_ir_opt() {
    without_ir_opt(test_append_greptile_plain_multilevel_key_order);
}

/// Mutating the selected child before retaining a copy cancels, including at a growth threshold.
#[test]
fn test_append_grok_review_prefilled_detached_child_control() {
    let entries = (0..12).map(|index| format!("\"p{index}\":1")).collect::<Vec<_>>().join(",");
    let expected = format!("W:null:[{{\"other\":2}}]:{{{entries}}}");
    assert_output(r#"<?php
function run(array $items): void {
    $copy = [];
    set_error_handler(function(int $level, string $message) use (&$items, &$copy): bool {
        echo 'W:';
        for ($i = 0; $i < 12; $i++) { $items[0]["p$i"] = 1; }
        $copy = $items[0]; $items[0] = ['other' => 2]; return true;
    });
    $result = ($items[]['k'] += 1);
    restore_error_handler();
    echo json_encode($result), ':', json_encode($items), ':', json_encode($copy);
    unset($items, $copy, $result);
}
run([]);
"#, &expected);
}

/// Callback child separation still cancels without EIR optimization.
#[test]
fn test_append_grok_review_prefilled_detached_child_control_no_ir_opt() {
    without_ir_opt(test_append_grok_review_prefilled_detached_child_control);
}

/// Checks one callback/operator pair and verifies complete temporary cleanup.
fn assert_append(callback: &str, operation: &str, expected: &str) {
    let source = format!(r#"<?php
$items = []; $copy = [];
set_error_handler(function(int $level, string $message) use (&$items, &$copy): bool {{
    echo 'W:'; {callback} return true;
}});
try {{ $result = ($items[]['k'] {operation}); echo json_encode($result), ':'; }}
catch (DivisionByZeroError $e) {{ echo 'division:'; }}
restore_error_handler(); echo json_encode($items), ':', json_encode($copy);
unset($items, $copy, $result, $e);
"#);
    assert_output(&source, expected);
}

/// Runs a single fixture while checking its independent result and heap ownership.
fn assert_output(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Add assignment yields null once its warning callback separates the selected child.
#[test]
fn test_append_eighth_review_separated_add() {
    assert_append("$items[0]['other'] = 2;", "+= 1", r#"W:null:[{"other":2}]:[]"#);
}

/// Separated addition is unchanged without EIR optimization.
#[test]
fn test_append_eighth_review_separated_add_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_separated_add);
}

/// Concat assignment yields null rather than evaluating the operator on the detached child.
#[test]
fn test_append_eighth_review_separated_concat() {
    assert_append("$items[0]['other'] = 2;", ".= 'x'", r#"W:null:[{"other":2}]:[]"#);
}

/// Separated concatenation is unchanged without EIR optimization.
#[test]
fn test_append_eighth_review_separated_concat_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_separated_concat);
}

/// Cancellation prevents division by zero from running at all.
#[test]
fn test_append_eighth_review_separated_division() {
    assert_append("$items[0]['other'] = 2;", "/= 0", r#"W:null:[{"other":2}]:[]"#);
}

/// Cancelled division is unchanged without EIR optimization.
#[test]
fn test_append_eighth_review_separated_division_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_separated_division);
}

/// Adding a sibling at the root does not detach the selected nested container.
#[test]
fn test_append_eighth_review_root_sibling_control() {
    assert_append("$items[] = 'changed';", "+= 1", r#"W:1:[{"k":1},"changed"]:[]"#);
}

/// Unrelated root appends do not cancel unoptimized operations either.
#[test]
fn test_append_eighth_review_root_sibling_control_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_root_sibling_control);
}

/// A still-attached division retains its ordinary DivisionByZeroError.
#[test]
fn test_append_eighth_review_live_division_control() {
    assert_append("$items[] = 'changed';", "/= 0", r#"W:division:[{"k":null},"changed"]:[]"#);
}

/// Live division errors do not depend on optimization.
#[test]
fn test_append_eighth_review_live_division_control_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_live_division_control);
}

/// A handler with no mutation leaves the ordinary compound update active.
#[test]
fn test_append_eighth_review_nonmutating_control() {
    assert_append("", "+= 1", r#"W:1:[{"k":1}]:[]"#);
}

/// Nonmutating callbacks keep unoptimized updates active.
#[test]
fn test_append_eighth_review_nonmutating_control_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_nonmutating_control);
}

/// A retained old root copy keeps the selected container alive for the update.
#[test]
fn test_append_eighth_review_retained_old_copy() {
    assert_append("$copy = $items; $items[0]['other'] = 2;", "+= 1", r#"W:1:[{"other":2}]:[{"k":1}]"#);
}

/// Root-copy ownership also preserves unoptimized updates.
#[test]
fn test_append_eighth_review_retained_old_copy_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_retained_old_copy);
}

/// A separately boxed child payload keeps the operator active after its original box detaches.
#[test]
fn test_append_eighth_review_retained_child_copy() {
    assert_output(r#"<?php
function run(array $items): void {
    $copy = [];
    set_error_handler(function(int $level, string $message) use (&$items, &$copy): bool {
        echo 'W:'; $copy = $items[0]; $items[0]['other'] = 2; return true;
    });
    $result = ($items[]['k'] += 1);
    restore_error_handler();
    $copy[] = 3;
    echo json_encode($result), ':', json_encode($items), ':', json_encode($copy);
}
run([]);
"#, r#"W:1:[{"other":2}]:{"k":1,"0":3}"#);
}

/// Child payload ownership is preserved without EIR optimization.
#[test]
fn test_append_eighth_review_retained_child_copy_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_retained_child_copy);
}

/// A payload copy alone does not keep the still-attached pending child writable.
#[test]
fn test_append_ninth_review_retained_child_without_root_separation() {
    assert_output(r#"<?php
function run(array $items): void {
    $copy = [];
    set_error_handler(function(int $level, string $message) use (&$items, &$copy): bool {
        echo 'W:'; $copy = $items[0]; return true;
    });
    $result = ($items[]['k'] += 1);
    restore_error_handler();
    $copy[] = 3;
    echo json_encode($result), ':', json_encode($items), ':', json_encode($copy);
}
run([]);
"#, r#"W:null:[[]]:[3]"#);
}

/// Payload-only retention keeps ordinary cancellation semantics without EIR optimization.
#[test]
fn test_append_ninth_review_retained_child_without_root_separation_no_ir_opt() {
    without_ir_opt(test_append_ninth_review_retained_child_without_root_separation);
}

/// Numeric dimensions preserve the selected indexed payload when the root separates.
#[test]
fn test_append_ninth_review_retained_indexed_child_copy() {
    assert_output(r#"<?php
function run(array $items): void {
    $copy = [];
    set_error_handler(function(int $level, string $message) use (&$items, &$copy): bool {
        echo 'W:'; $copy = $items[0]; $items[0]['other'] = 2; return true;
    });
    $result = ($items[][0] += 1);
    restore_error_handler();
    $copy[] = 3;
    echo json_encode($result), ':', json_encode($items), ':', json_encode($copy);
}
run([]);
"#, r#"W:1:[{"other":2}]:[1,3]"#);
}

/// Indexed payload retention is unchanged without EIR optimization.
#[test]
fn test_append_ninth_review_retained_indexed_child_copy_no_ir_opt() {
    without_ir_opt(test_append_ninth_review_retained_indexed_child_copy);
}

/// Independent cell and payload survivors together mean COW invalidated the pending target.
#[test]
fn test_append_ninth_review_retained_root_and_child_copies() {
    assert_output(r#"<?php
$items = []; $root = []; $child = [];
set_error_handler(function(int $level, string $message) use (&$items, &$root, &$child): bool {
    echo 'W:'; $root = $items; $child = $items[0]; $items[0]['other'] = 2; return true;
});
$result = ($items[]['k'] += 1);
restore_error_handler();
echo json_encode($result), ':', json_encode($items), ':', json_encode($root), ':', json_encode($child);
"#, r#"W:null:[{"other":2}]:[[]]:[]"#);
}

/// Combined cell and payload survival cancels identically without EIR optimization.
#[test]
fn test_append_ninth_review_retained_root_and_child_copies_no_ir_opt() {
    without_ir_opt(test_append_ninth_review_retained_root_and_child_copies);
}

/// Multiple independent payload owners cancel rather than jointly receiving a detached write.
fn assert_multiple_child_copies(dimension: &str) {
    for copies in [2, 3] {
        let source = format!(r#"<?php
$items = []; $first = []; $second = []; $third = [];
set_error_handler(function(int $level, string $message) use (&$items, &$first, &$second, &$third): bool {{
    echo 'W:';
    $first = $items[0];
    $second = $items[0];
    if ({copies} === 3) {{ $third = $items[0]; }}
    $items[0] = ['other' => 2];
    return true;
}});
$result = ($items[][{dimension}] += 1);
restore_error_handler();
echo json_encode($result), ':', json_encode($items), ':', json_encode($first), ':', json_encode($second), ':', json_encode($third);
unset($items, $first, $second, $third, $result);
"#);
        assert_output(&source, r#"W:null:[{"other":2}]:[]:[]:[]"#);
    }
}

/// Two and three copies must remain unchanged when a detached string-key append cancels.
#[test]
fn test_append_grok_review_multiple_child_copies() {
    assert_multiple_child_copies("'k'");
}

/// String-key cancellation counts every owner even without EIR optimization.
#[test]
fn test_append_grok_review_multiple_child_copies_no_ir_opt() {
    without_ir_opt(test_append_grok_review_multiple_child_copies);
}

/// Integer-key append cancellation observes the same exact owner-count requirement.
#[test]
fn test_append_grok_review_multiple_indexed_child_copies() {
    assert_multiple_child_copies("0");
}

/// Integer-key cancellation counts every owner even without EIR optimization.
#[test]
fn test_append_grok_review_multiple_indexed_child_copies_no_ir_opt() {
    without_ir_opt(test_append_grok_review_multiple_indexed_child_copies);
}

/// Shared root copies retain one child slot, not two independent child payload owners.
#[test]
fn test_append_grok_review_multiple_root_copies_control() {
    assert_output(r#"<?php
$items = []; $first = []; $second = [];
set_error_handler(function(int $level, string $message) use (&$items, &$first, &$second): bool {
    echo 'W:'; $first = $items; $second = $items;
    $items[0] = ['other' => 2]; return true;
});
$result = ($items[][0] += 1);
restore_error_handler();
echo json_encode($result), ':', json_encode($items), ':', json_encode($first), ':', json_encode($second);
unset($items, $first, $second, $result);
"#, r#"W:1:[{"other":2}]:[[1]]:[[1]]"#);
}

/// Shared root storage still keeps the selected child writable without EIR optimization.
#[test]
fn test_append_grok_review_multiple_root_copies_control_no_ir_opt() {
    without_ir_opt(test_append_grok_review_multiple_root_copies_control);
}

/// Prefix increment returns the written scalar when its pending append is cancelled.
#[test]
fn test_append_ninth_review_separated_prefix_increment() {
    assert_output(r#"<?php
$items = [];
set_error_handler(function(int $level, string $message) use (&$items): bool {
    echo 'W:'; $items[0]['other'] = 2; return true;
});
$result = ++$items[]['k'];
restore_error_handler(); echo json_encode($result), ':', json_encode($items);
"#, r#"W:1:[{"other":2}]"#);
}

/// Cancelled prefix increment preserves its result without EIR optimization.
#[test]
fn test_append_ninth_review_separated_prefix_increment_no_ir_opt() {
    without_ir_opt(test_append_ninth_review_separated_prefix_increment);
}

/// Every missing intermediate append dimension emits its ordinary PHP warning.
#[test]
fn test_append_ninth_review_missing_intermediate_warnings() {
    assert_output(r#"<?php
$items = [];
set_error_handler(function(int $level, string $message): bool {
    echo 'W:', $message, '|'; return true;
});
$result = ($items[]['k']['other'][] += 1);
restore_error_handler(); echo json_encode($result), ':', json_encode($items);
"#, r#"W:Undefined array key "k"|W:Undefined array key "other"|1:[{"k":{"other":[1]}}]"#);
}

/// Intermediate append diagnostics are unchanged without EIR optimization.
#[test]
fn test_append_ninth_review_missing_intermediate_warnings_no_ir_opt() {
    without_ir_opt(test_append_ninth_review_missing_intermediate_warnings);
}

/// Replacing the root loses the selected container even if the replacement is value-equal.
#[test]
fn test_append_eighth_review_equal_root_replacement() {
    assert_append("$items = [[]];", "+= 1", "W:null:[[]]:[]");
}

/// Value-equal replacements cancel without EIR optimization.
#[test]
fn test_append_eighth_review_equal_root_replacement_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_equal_root_replacement);
}

/// A missing-key handler sees an empty attached parent and no prematurely inserted leaf.
#[test]
fn test_append_eighth_review_callback_parent_state() {
    assert_append("echo json_encode($items), '|'; $items[0]['other'] = 2;", "+= 1",
        r#"W:[[]]|null:[{"other":2}]:[]"#);
}

/// Callback parent publication retains the same order without optimization.
#[test]
fn test_append_eighth_review_callback_parent_state_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_callback_parent_state);
}

/// Object-property array roots cancel without replaying or redirecting the captured receiver.
#[test]
fn test_append_eighth_review_object_property_root() {
    assert_output(r#"<?php
class Box { public array $items = []; }
$box = new Box();
set_error_handler(function(int $level, string $message) use ($box): bool {
    echo 'W:'; $box->items[0]['other'] = 2; return true;
});
$result = ($box->items[]['k'] += 1);
restore_error_handler(); echo json_encode($result), ':', json_encode($box->items);
unset($box, $result);
"#, r#"W:null:[{"other":2}]"#);
}

/// Property-root cancellation also works without optimization.
#[test]
fn test_append_eighth_review_object_property_root_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_object_property_root);
}

/// Static-property array roots use the same pending cell liveness rule.
#[test]
fn test_append_eighth_review_static_property_root() {
    assert_output(r#"<?php
class Box { public static array $items = []; }
set_error_handler(function(int $level, string $message): bool {
    echo 'W:'; Box::$items[0]['other'] = 2; return true;
});
$result = (Box::$items[]['k'] += 1);
restore_error_handler(); echo json_encode($result), ':', json_encode(Box::$items);
Box::$items = []; unset($result);
"#, r#"W:null:[{"other":2}]"#);
}

/// Static-root cancellation also works without optimization.
#[test]
fn test_append_eighth_review_static_property_root_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_static_property_root);
}

/// Cancelled concatenation skips object string conversion while retaining balanced RHS cleanup.
#[test]
fn test_append_eighth_review_owned_rhs_conversion() {
    assert_output(r#"<?php
class Text { public function __toString(): string { echo 'converted'; return 'x'; } }
$items = [];
set_error_handler(function(int $level, string $message) use (&$items): bool {
    echo 'W:'; $items[0]['other'] = 2; return true;
});
$result = ($items[]['k'] .= new Text());
restore_error_handler(); echo json_encode($result), ':', json_encode($items);
unset($items, $result);
"#, r#"W:null:[{"other":2}]"#);
}

/// Owned RHS cancellation remains balanced without optimization.
#[test]
fn test_append_eighth_review_owned_rhs_conversion_no_ir_opt() {
    without_ir_opt(test_append_eighth_review_owned_rhs_conversion);
}
