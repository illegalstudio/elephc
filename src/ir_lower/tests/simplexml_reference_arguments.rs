//! Purpose:
//! Covers native XML reference arguments and their independent cells on every target ABI.
//!
//! Called from:
//! - AST-to-EIR emitter CI.
//!
//! Key details:
//! - Writable selections materialize missing nodes, and never become property writeback plans.

/// Reference arguments select writable views before entering the ordinary managed-cell ABI.
fn verify_simplexml_reference_argument_write_selections(target: &str) {
    let source = r#"<?php
function replaceTargetXmlSelection(&$value): void { $value = 'replacement'; }
function passTargetXmlSelections(SimpleXMLElement $xml, string $name): void {
    replaceTargetXmlSelection($xml->missing);
    replaceTargetXmlSelection($xml->nested->child);
    replaceTargetXmlSelection($xml->{$name});
    replaceTargetXmlSelection($xml->p[0]);
    settype(var: $xml->q, type: 'integer');
}
function passErasedTargetXmlSelections(mixed $xml): void {
    if ($xml instanceof SimpleXMLElement) {
        replaceTargetXmlSelection($xml[0]);
    }
}
function passRuntimeTargetXmlSelections(mixed $callback, SimpleXMLElement $xml): void {
    $callback($xml->runtime);
    $callback(value: $xml->runtimeNamed[0]);
}
$xml = new SimpleXMLElement('<r><p>old</p><q>4</q></r>');
passTargetXmlSelections($xml, 'p');
passErasedTargetXmlSelections($xml);
passRuntimeTargetXmlSelections(replaceTargetXmlSelection(...), $xml);
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "passTargetXmlSelections").unwrap();
    assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::MethodCall
        && matches!(inst.immediate, Some(crate::ir::Immediate::Data(id))
            if module.data.strings[id.as_raw() as usize] == "__elephc_property_reference_fetch")),
        "{target}: reference binding must select in write context");
    assert!(!function.instructions.iter().any(|inst| inst.op == crate::ir::Op::PropSet),
        "{target}: replacing an independent reference must not write the XML property back");
    let erased = module.functions.iter().find(|function| function.name == "passErasedTargetXmlSelections").unwrap();
    assert!(erased.instructions.iter().any(|inst| inst.op == crate::ir::Op::MethodCall
        && matches!(inst.immediate, Some(crate::ir::Immediate::Data(id))
            if module.data.strings[id.as_raw() as usize] == "__elephc_dimension_select")),
        "{target}: checked XML dimensions must retain write context in boxed storage");
    let runtime = module.functions.iter().find(|function| function.name == "passRuntimeTargetXmlSelections").unwrap();
    assert!(module.required_runtime_features.eval_bridge,
        "{target}: runtime passing-mode metadata requires the bridge even without source eval");
    assert!(runtime.instructions.iter().any(|inst| inst.op == crate::ir::Op::RuntimeCall
        && inst.immediate == Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::CallableParameterIsByRef))),
        "{target}: resolve passing modes before choosing the XML read context");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains("__elephc_eval_callable_parameter_is_by_ref"), "{target}: emit the passing-mode bridge");
}

supported_target_cases!(simplexml_reference_argument_write_selections, verify_simplexml_reference_argument_write_selections);

/// A closed world answers the passing-mode query natively: no eval context is declared, so no
/// function carries an `EvalContext` local and the interpreter bridge is not requested.
fn verify_closed_world_passing_mode_query_declares_no_eval_context(target: &str) {
    let source = r#"<?php
function mkXml() { return new SimpleXMLElement('<r><q>1</q></r>'); }
function viaCallable(callable $callback, array $items, object $holder): void {
    $callback($items['k']);
    $callback($holder->p);
    $callback(string: $items['n']);
}
function viaClosureVariable(): void {
    $f = function (&$value) { $value .= 'x'; };
    $items = ['k' => 'a'];
    $f($items['k']);
}
function viaMixed(mixed $callback, mixed $xml): void { $callback($xml->q); }
function sorted(array $rows, callable $by): array {
    usort($rows, fn($a, $b) => $by($a['n'], $b['n']));
    uasort($rows, fn($a, $b) => $a['n'] <=> $b['n']);
    return $rows;
}
sorted([['n' => 2], ['n' => 1]], fn($x, $y) => $x <=> $y);
viaCallable('strtoupper', ['k' => 'a', 'n' => 'b'], (object) ['p' => 'c']);
viaClosureVariable();
viaMixed(fn($v) => $v, mkXml());
"#;
    crate::closed_world::set_enabled(true);
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    crate::closed_world::set_enabled(false);
    for function in &module.functions {
        assert!(!function.locals.iter().any(|local| matches!(local.kind, crate::ir::LocalKind::EvalContext)),
            "{target}: {} must declare no eval context in a closed world", function.name);
    }
    let query = module.functions.iter().find(|function| function.name == "viaMixed").unwrap();
    assert!(query.instructions.iter().any(|inst| inst.op == crate::ir::Op::RuntimeCall
        && inst.immediate == Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::CallableParameterIsByRef))),
        "{target}: the passing-mode query is still emitted");
    assert!(!module.required_runtime_features.eval_bridge, "{target}: no interpreter requested");
}

supported_target_cases!(closed_world_passing_mode_query_declares_no_eval_context, verify_closed_world_passing_mode_query_declares_no_eval_context);
