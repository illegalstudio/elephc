//! Purpose:
//! Verifies callback-owner retirement at output-buffer closure across native and eval calls.
//!
//! Called from:
//! - The focused codegen I/O integration harness.
//!
//! Key details:
//! - Observable destructor timing is checked separately from the final native heap summary.

use crate::support::*;

const OWNER: &str = r#"
class BufferOwner {
    public function __invoke(string $buffer, int $phase): string { return $buffer; }
    public function __destruct() { echo "released:"; }
}
"#;
const CLOSE: &str = r#"
$handler = new BufferOwner();
ob_start($handler);
unset($handler);
echo "discarded";
ob_end_clean();
echo "depth=", ob_get_level(), "\n";
"#;

/// Requires correct callback timing and no outstanding native allocation after program cleanup.
fn clean(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Keeps a fragment opaque to compile-time eval lowering without changing its PHP contents.
fn dynamic(body: &str) -> String {
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("$source = $argc > 0 ? '{quoted}' : ''; eval($source);")
}

/// Retires a native closure's captured object when the buffer releases its descriptor.
#[test]
fn test_output_handler_lifecycle_native_owner() {
    let close = CLOSE.replace("ob_start($handler);", r#"
$callback = function(string $buffer, int $phase) use ($handler): string {
    return $handler->__invoke($buffer, $phase);
};
ob_start($callback);
unset($callback);
"#);
    clean(&format!("<?php {OWNER} {close}"), "released:depth=0\n");
}

/// Retires a native callable registered and closed from opaque eval before later eval output.
#[test]
fn test_output_handler_lifecycle_eval_native_owner() {
    clean(&format!("<?php {OWNER} {}", dynamic(CLOSE)), "released:depth=0\n");
}

/// Runs an eval-declared destructor at buffer closure while its declaration context remains live.
#[test]
fn test_output_handler_lifecycle_eval_declared_owner() {
    clean(&format!("<?php {}", dynamic(&format!("{OWNER} {CLOSE}"))), "released:depth=0\n");
}

/// Propagates a retiring eval callback's native destructor exception through the protected output call.
#[test]
fn test_output_handler_lifecycle_eval_destructor_throw() {
    let owner = OWNER.replace("echo \"released:\";", "throw new Exception(\"closed\");");
    let close = CLOSE.replace("ob_end_clean();",
        "try { ob_end_clean(); } catch (Throwable $e) { echo $e->getMessage(), \":\"; }");
    clean(&format!("<?php {owner} {}", dynamic(&close)), "closed:depth=0\n");
}

/// Releases the saved raw result if an eval-registered callback destructor interrupts get-and-pop.
#[test]
fn test_output_handler_lifecycle_eval_get_pop_destructor_throw() {
    let owner = OWNER.replace("echo \"released:\";", "throw new Exception(\"closed\");");
    for (operation, prefix) in [("ob_get_clean", ""), ("ob_get_flush", "discarded")] {
        let close = CLOSE.replace("ob_end_clean();", &format!(
            "try {{ {operation}(); }} catch (Throwable $e) {{ echo $e->getMessage(), \":\"; }}"));
        clean(&format!("<?php {owner} {}", dynamic(&close)), &format!("{prefix}closed:depth=0\n"));
    }
}

/// Protects native get-and-pop results while releasing a closure's captured object.
#[test]
fn test_output_handler_lifecycle_native_get_pop_destructor_throw() {
    let owner = OWNER.replace("echo \"released:\";", "throw new Exception(\"closed\");");
    let close = CLOSE.replace("ob_start($handler);", r#"
$callback = function(string $buffer, int $phase) use ($handler): string { return $buffer; };
ob_start($callback);
unset($callback);
"#);
    for (operation, prefix) in [("ob_get_clean", ""), ("ob_get_flush", "discarded")] {
        let close = close.replace("ob_end_clean();", &format!(
            "try {{ {operation}(); }} catch (Throwable $e) {{ echo $e->getMessage(), \":\"; }}"));
        clean(&format!("<?php {owner} {close}"), &format!("{prefix}closed:depth=0\n"));
    }
}

/// Keeps repeated builtin registrations balanced while one eval context remains alive.
#[test]
fn test_output_handler_lifecycle_repeated_mbstring() {
    let operation = r#"
    ob_start("mb_output_handler");
    echo "discarded";
    ob_end_clean();
"#;
    let body = format!("mb_http_output(\"UTF-8\"); {} echo ob_get_level(), \"\\n\";", operation.repeat(128));
    clean(&format!("<?php {}", dynamic(&body)), "0\n");
}
