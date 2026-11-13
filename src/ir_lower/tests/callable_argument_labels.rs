//! Purpose:
//! Checks source callable names on caller-side nullable native-object guards.
//!
//! Called from:
//! - The all-target AST-to-EIR regression suite.
//!
//! Key details:
//! - Runtime argument containers reach native object guards after static checking.
//! - The emitted diagnostic must identify that closure or first-class function.

/// Every target retains callable labels before argument coercion emits an exception.
fn verify_callable_argument_labels(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
function labelMaybe(bool $present): mixed {
    return $present ? new SimpleXMLElement('<r/>') : null;
}
function labelFunction(SimpleXMLElement $node): void {}
function labelArguments(mixed $node): array { return [$node]; }
$missing = labelMaybe($argc < 0);
$plain = function(SimpleXMLElement $node): void {};
$generator = function(SimpleXMLElement $node): Generator { yield 1; };
$function = labelFunction(...);
$arguments = labelArguments($missing);
call_user_func_array($plain, $arguments);
call_user_func_array($generator, $arguments);
call_user_func_array($function, $arguments);
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
