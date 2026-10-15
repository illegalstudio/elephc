//! Purpose:
//! Heap regressions for SPL offset rejection messages outside concat scratch capacity.
//!
//! Called from:
//! - The codegen integration harness through `codegen::spl`.
//!
//! Key details:
//! - Large class names force both intermediate message concatenations onto the heap.

use crate::support::compile_and_run_with_heap_debug;

/// Repeated caught offset errors retire large intermediate and final message allocations.
#[test]
fn test_spl_offset_large_class_error_message_is_heap_clean() {
    let class_name = format!("Offset{}", "X".repeat(65_536));
    let source = format!(r#"<?php
class {class_name} {{}}
$offset = new {class_name}();
    $fixed = new SplFixedArray(1);
    $list = new SplDoublyLinkedList();
    for ($i = 0; $i < 3; $i++) {{
        try {{ $fixed->offsetSet($offset, 1); }}
        catch (TypeError $error) {{ echo get_class($error), ":", strlen($error->getMessage()), "\n"; unset($error); }}
        try {{ $list->offsetSet($offset, 1); }}
        catch (TypeError $error) {{ echo get_class($error), ":", strlen($error->getMessage()), "\n"; unset($error); }}
    }}
    unset($fixed, $list);

unset($offset);
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "TypeError:65588\nTypeError:65625\n".repeat(3));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
