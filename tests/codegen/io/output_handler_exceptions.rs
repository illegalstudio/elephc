//! Purpose:
//! Checks output-handler exceptions, disabled callbacks, and completed buffer state against PHP.
//!
//! Called from:
//! - The focused codegen I/O integration harness.
//!
//! Key details:
//! - Every explicit clean/flush/end/get action is exercised through native and opaque eval calls.
//! - Native heap summaries verify callback argument, result, and buffer owners after exception cleanup.

use crate::support::*;

const HANDLER: &str = r#"
function throwing_output_handler(string $bytes, int $phase): string {
    throw new Exception("handler");
}
"#;

/// Makes eval source opaque to compile-time lowering while preserving its runtime PHP contents.
fn dynamic(body: &str) -> String {
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("$source = $argc > 0 ? '{quoted}' : ''; eval($source);")
}

/// Requires both PHP-visible operation completion and balanced native ownership.
fn clean(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

#[derive(Clone, Copy)]
struct OutputOperation {
    name: &'static str,
    flush: bool,
    retained: bool,
}

const OUTPUT_OPERATIONS: [OutputOperation; 6] = [
    OutputOperation { name: "ob_clean", flush: false, retained: true },
    OutputOperation { name: "ob_flush", flush: true, retained: true },
    OutputOperation { name: "ob_end_clean", flush: false, retained: false },
    OutputOperation { name: "ob_end_flush", flush: true, retained: false },
    OutputOperation { name: "ob_get_clean", flush: false, retained: false },
    OutputOperation { name: "ob_get_flush", flush: true, retained: false },
];

/// Generates one public operation and checks the state observed by its catch block.
fn check_operation(operation: OutputOperation, eval: bool, eval_declaration: bool) {
    let inspect = if operation.retained { r#"
    $contents = ob_get_contents();
    $status = ob_get_status();
    $flag_key = "flags";
    $flags = $status[$flag_key];
    ob_end_clean();
    echo $caught, ":", $level, ":", $contents, ":", $flags, "\n";
"# } else { r#"echo $caught, ":", $level, "::-1\n";"# };
    let body = format!(r#"
ob_start("throwing_output_handler");
echo "before";
try {{ {operation}(); echo "missed"; }} catch (Throwable $error) {{
    $caught = $error->getMessage();
    $level = ob_get_level();
    {inspect}
}}
ob_start();
echo "after";
$after = ob_get_clean();
echo $after, "\n";
"#, operation = operation.name);
    let source = if eval_declaration {
        format!("<?php {}", dynamic(&format!("{HANDLER}{body}")))
    } else if eval {
        format!("<?php {HANDLER} {}", dynamic(&body))
    } else {
        format!("<?php {HANDLER} {body}")
    };
    let prefix = if operation.flush { "before" } else { "" };
    let state = if operation.retained { "1::12401" } else { "0::-1" };
    clean(&source, &format!("{prefix}handler:{state}\nafter\n"));
}

/// Generates each public operation and checks the state observed by its catch block.
fn check_operations(eval: bool, eval_declaration: bool) {
    for operation in OUTPUT_OPERATIONS {
        check_operation(operation, eval, eval_declaration);
    }
}

/// Completes native operations and releases uniform-invoker arguments when a handler throws.
#[test]
fn test_output_handler_exceptions_native_operations() {
    check_operations(false, false);
}

macro_rules! eval_operation_test {
    ($name:ident, $index:expr, $eval_declaration:expr) => {
        /// Checks one bounded eval output operation so the per-test CI timeout covers one compile.
        #[test]
        fn $name() {
            check_operation(OUTPUT_OPERATIONS[$index], true, $eval_declaration);
        }
    };
}

eval_operation_test!(test_output_handler_exceptions_eval_native_ob_clean, 0, false);
eval_operation_test!(test_output_handler_exceptions_eval_native_ob_flush, 1, false);
eval_operation_test!(test_output_handler_exceptions_eval_native_ob_end_clean, 2, false);
eval_operation_test!(test_output_handler_exceptions_eval_native_ob_end_flush, 3, false);
eval_operation_test!(test_output_handler_exceptions_eval_native_ob_get_clean, 4, false);
eval_operation_test!(test_output_handler_exceptions_eval_native_ob_get_flush, 5, false);
eval_operation_test!(test_output_handler_exceptions_eval_declared_ob_clean, 0, true);
eval_operation_test!(test_output_handler_exceptions_eval_declared_ob_flush, 1, true);
eval_operation_test!(test_output_handler_exceptions_eval_declared_ob_end_clean, 2, true);
eval_operation_test!(test_output_handler_exceptions_eval_declared_ob_end_flush, 3, true);
eval_operation_test!(test_output_handler_exceptions_eval_declared_ob_get_clean, 4, true);
eval_operation_test!(test_output_handler_exceptions_eval_declared_ob_get_flush, 5, true);

/// A false-returning callback is disabled and must not process later buffer contents.
#[test]
fn test_output_handler_exceptions_false_disables_handler() {
    let handler = r#"
function disabled_output_handler(string $bytes, int $phase): bool {
    return $bytes !== "first";
}
"#;
    let body = r#"
ob_start("disabled_output_handler");
echo "first";
ob_flush();
$status = ob_get_status();
echo "second";
ob_end_flush();
$flag_key = "flags";
$flags = $status[$flag_key];
echo ":", $flags, "\n";
"#;
    clean(&format!("<?php {handler}{body}"), "firstsecond:12401\n");
    clean(&format!("<?php {handler} {}", dynamic(body)), "firstsecond:12401\n");
    let declared = handler.replace("return $bytes !== \"first\";", "return false;");
    clean(&format!("<?php {}", dynamic(&format!("{declared}{body}"))), "firstsecond:12401\n");
}

/// Preserves permission bits while exposing actual status and balanced simple, full, and empty results.
#[test]
fn test_output_handler_exceptions_status_ownership_and_input_flags() {
    let handler = r#"
function status_output_handler(string $bytes, int $phase): string { return $bytes; }
"#;
    let body = r#"
ob_start();
ob_start("status_output_handler", 0, 32767);
$before = ob_get_status(true);
echo "x";
ob_flush();
$after = ob_get_status(true);
ob_end_clean();
ob_end_clean();
$index = 1;
$key = "flags";
$first = $before[$index];
$last = $after[$index];
$before_flags = $first[$key];
$after_flags = $last[$key];
echo $before_flags, ":", $after_flags, ":";
$none = ob_get_status();
$count = count($none);
echo $count, "\n";
"#;
    clean(&format!("<?php {handler}{body}"), "4081:24561:0\n");
    clean(&format!("<?php {handler} {}", dynamic(body)), "4081:24561:0\n");
}
