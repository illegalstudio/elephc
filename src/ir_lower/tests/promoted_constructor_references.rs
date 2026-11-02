//! Purpose:
//! Keeps dynamic promoted-reference constructors reachable with hidden argument collectors.
//!
//! Called from:
//! - The supported-target AST-to-EIR suite.
//!
//! Key details:
//! - Backtrace capture adds a collector that generic reference padding thunks cannot forward.
//! - Both positional and named calls must use the ordinary fixed-class reference planner.

use crate::ir::Op;

/// Dynamic inherited constructors retain reference bindings despite backtrace-only parameters.
fn verify_promoted_constructor_hidden_collector(target: &str) {
    let source = r#"<?php
class HiddenCollectorReference {
    public function __construct(public mixed &$value) {}
}
class HiddenCollectorReferenceChild extends HiddenCollectorReference {}
function buildHiddenCollectorReference(bool $flag): void {
    $value = ['old'];
    $class = $flag ? 'HiddenCollectorReference' : 'HiddenCollectorReferenceChild';
    $positional = new $class($value);
    $named = new $class(value: $value);
    $value = ['new'];
    echo json_encode($positional->value), json_encode($named->value);
}
debug_backtrace();
buildHiddenCollectorReference($argc > 0);
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let constructor = &module.class_infos["HiddenCollectorReference"].methods["__construct"];
    assert!(crate::func_args::sig_collects_surplus_args(constructor),
        "{target}: backtrace capture must exercise the hidden constructor collector");
    let factory = module.functions.iter().find(|function|
        function.name == "buildHiddenCollectorReference").unwrap();
    let constructors = factory.instructions.iter().filter(|inst| inst.op == Op::ObjectNew)
        .collect::<Vec<_>>();
    assert_eq!(constructors.len(), 4,
        "{target}: both call shapes must select the parent and inherited constructor branches");
    assert!(constructors.iter().all(|inst| inst.operands.len() == constructor.params.len()),
        "{target}: every constructor branch must supply its hidden collector");
    assert!(factory.instructions.iter().any(|inst| inst.op == Op::PromoteLocalRefCell),
        "{target}: the caller binding must survive constructor frame retirement");
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

supported_target_cases!(promoted_constructor_hidden_collector,
    verify_promoted_constructor_hidden_collector);
