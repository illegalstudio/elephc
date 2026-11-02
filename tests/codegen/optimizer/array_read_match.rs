//! Purpose:
//! Preserve missing-value provenance when AST pruning selects a constant match arm.
//! Cover both an explicit arm and the default before EIR merge lowering.
//!
//! Called from:
//! - The optimizer group in the codegen integration harness.
//!
//! Key details:
//! - Type probes and caught TypeErrors must agree with PHP and leave no heap owners.

use crate::support::compile_and_run_with_heap_debug;

/// Constant match pruning retains the merge context required by a missing object read.
#[test]
fn test_constant_match_array_misses_keep_null_provenance() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class Box { public int $n = 7; }
$boxes = [0 => new Box()];
$a = match (true) { true => $boxes[3], default => new Box() };
$b = match (false) { true => new Box(), default => @$boxes[3] };
echo gettype($a), " ";
try { echo get_class($a); } catch (TypeError $e) { echo "TypeError\n"; }
echo gettype($b), " ";
try { echo get_class($b); } catch (TypeError $e) { echo "TypeError\n"; }
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "NULL TypeError\nNULL TypeError\n", "{}", output.stderr);
    assert_eq!(output.stderr.matches("Warning:").count(), 1, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
