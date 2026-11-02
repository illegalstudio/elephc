//! Purpose:
//! Checks source callable names on caller-side nullable native-object guards.
//!
//! Called from:
//! - The all-target AST-to-EIR regression suite.
//!
//! Key details:
//! - Static closure calls can reject an argument before entering the closure body.
//! - The emitted diagnostic must identify that closure or first-class function.

/// Every target retains callable labels before argument coercion emits an exception.
fn verify_callable_argument_labels(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
function labelMaybe(bool $present): ?SimpleXMLElement {
    return $present ? new SimpleXMLElement('<r/>') : null;
}
function labelFunction(SimpleXMLElement $node): void {}
$missing = labelMaybe($argc < 0);
$plain = function(SimpleXMLElement $node): void {};
$generator = function(SimpleXMLElement $node): Generator { yield 1; };
$function = labelFunction(...);
$plain($missing); $generator($missing); $function($missing);
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let labels = &module.data.strings;
    assert!(!labels.iter().any(|label| label.starts_with("call(): Argument #1 ($node)")),
        "{target}: a statically known callable must not use the anonymous call fallback");
    assert!(labels.iter().any(|label| label.starts_with("{closure:") && label.contains("Argument #1 ($node)")),
        "{target}: preserve the closure's declared source label");
    assert!(labels.iter().any(|label| label.starts_with("labelFunction(): Argument #1 ($node)")),
        "{target}: preserve the first-class function's PHP name");
}

supported_target_cases!(callable_argument_labels, verify_callable_argument_labels);
