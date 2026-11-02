//! Purpose: compare GLOBALS element semantics with the actual global symbol table.
//! Called from: the focused interpreter unit suite.
//! Key details: local shadows and nested writes must not turn GLOBALS into a local array.

use super::super::*;
use super::support::*;

#[test]
fn globals_array_quiet_checks_and_unset_use_global_storage() {
    let program = parse_fragment(br#"
$present = isset($GLOBALS['counter']);
$not_empty = !empty($GLOBALS['counter']);
$missing = empty($GLOBALS['absent']);
unset($GLOBALS['counter']);
$removed = !isset($GLOBALS['counter']);
"#).unwrap();
    let mut values = FakeOps::default();
    let mut globals = ElephcEvalScope::new();
    globals.set("counter", values.int(3).unwrap(), ScopeCellOwnership::Owned);
    let mut scope = ElephcEvalScope::new();
    let mut context = ElephcEvalContext::new();
    assert!(context.set_global_scope(&mut globals));
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    for name in ["present", "not_empty", "missing", "removed"] {
        assert!(values.truthy(scope.visible_cell(name).unwrap()).unwrap(), "{name}");
    }
    assert!(globals.visible_cell("counter").is_none());
    assert!(scope.visible_cell("GLOBALS").is_none());
}

#[test]
fn globals_array_reads_and_writes_preserve_local_shadowing() {
    let program = parse_fragment(br#"
$before = $GLOBALS['counter'];
$GLOBALS['counter'] = 7;
$after = $GLOBALS['counter'];
"#).unwrap();
    let mut values = FakeOps::default();
    let mut globals = ElephcEvalScope::new();
    globals.set("counter", values.int(3).unwrap(), ScopeCellOwnership::Owned);
    let mut scope = ElephcEvalScope::new();
    scope.set("counter", values.int(99).unwrap(), ScopeCellOwnership::Owned);
    let mut context = ElephcEvalContext::new();
    assert!(context.set_global_scope(&mut globals));

    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.raw_value_word(scope.visible_cell("before").unwrap()).unwrap(), 3);
    assert_eq!(values.raw_value_word(scope.visible_cell("after").unwrap()).unwrap(), 7);
    assert_eq!(values.raw_value_word(scope.visible_cell("counter").unwrap()).unwrap(), 99);
    assert_eq!(values.raw_value_word(globals.visible_cell("counter").unwrap()).unwrap(), 7);
    assert!(scope.visible_cell("GLOBALS").is_none(), "GLOBALS must not become a local variable");
}

#[test]
fn globals_array_nested_once_guard_uses_global_storage() {
    let program = parse_fragment(br#"
if (empty($GLOBALS['loaded']['one'])) {
    $GLOBALS['loaded']['one'] = true;
    $GLOBALS['runs'] += 1;
}
if (empty($GLOBALS['loaded']['one'])) {
    $GLOBALS['runs'] += 1;
}
"#).unwrap();
    let mut values = FakeOps::default();
    let mut globals = ElephcEvalScope::new();
    globals.set("runs", values.int(0).unwrap(), ScopeCellOwnership::Owned);
    let mut scope = ElephcEvalScope::new();
    let mut context = ElephcEvalContext::new();
    assert!(context.set_global_scope(&mut globals));

    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.raw_value_word(globals.visible_cell("runs").unwrap()).unwrap(), 1);
    assert!(globals.visible_cell("loaded").is_some());
    assert!(scope.visible_cell("GLOBALS").is_none());
}
