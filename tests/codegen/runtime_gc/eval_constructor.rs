//! Purpose:
//! Checks temporary ownership when opaque eval constructs and throws native exceptions.
//!
//! Called from:
//! - The codegen test binary's runtime GC module.
//!
//! Key details:
//! - Reused message and previous-exception cells must survive temporary argument cleanup.
//! - Comparing one and many repetitions detects retained boxes, strings, and default values.

use crate::support::*;

/// Balances literal, borrowed, named, omitted, and previous-Throwable constructor arguments in native eval.
#[test]
fn test_eval_constructor_temporary_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = r#"
try { throw new RuntimeException("literal", 3); } catch (RuntimeException) {}
try { throw new RuntimeException(message: $message, previous: $previous); } catch (RuntimeException) {}
try { throw new RuntimeException(); } catch (RuntimeException) {}
"#.repeat(count);
        let source = format!(r#"<?php
$source = $argc > 0 ? '
$message = "retained"; $previous = new RuntimeException("previous");
{calls}
echo $message;
' : '';
eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "retained");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "native eval constructor arguments retained owners");
}

/// Balances an owned Mixed reference slot both when a native constructor replaces it and when it leaves it intact.
#[test]
fn test_eval_constructor_mixed_reference_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = r#"
try { throw new RewriteEvalReference($value); } catch (Throwable) {}
try { throw new KeepEvalReference($value); } catch (Throwable) {}
"#.repeat(count);
        let source = format!(r#"<?php
class RewriteEvalReference extends Exception {{
    public function __construct(mixed &$value) {{ $value = 41; }}
}}
class KeepEvalReference extends Exception {{
    public function __construct(mixed &$value) {{}}
}}
$source = $argc > 0 ? '
$value = 30;
{calls}
echo $value;
' : '';
eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "41");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "native constructor Mixed reference slots retained owners");
}
