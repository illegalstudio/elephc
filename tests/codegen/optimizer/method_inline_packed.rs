//! Purpose:
//! Runs the sprite-batch regression with and without EIR optimization.
//!
//! Called from:
//! - The optimizer method-inlining codegen suite.
//!
//! Key details:
//! - Strict heap validation covers the successful writes and the caught cold throw.

use super::*;

/// Packed writes, post-increment and the capacity exception preserve both modes' behavior.
#[test]
fn test_method_packed_sprite_batch_writes_and_cold_throw() {
    assert_both_modes(
        include_str!("../../../examples/packed-sprite-batch/main.php"),
        false,
        "1,2:0.5:0,0\n9,10:0.5:1,1\nThe sprite batch holds at most 1 sprites.\n",
    );
}

/// Scoped roots are retired on every loop iteration and a caught throw leaves the counter intact.
#[test]
fn test_method_packed_guard_in_loop_preserves_state_and_scratch() {
    let source = r#"<?php
final class GuardCounter {
    private int $count = 0;
    private int $capacity = 3;
    private function reserve(): int {
        if ($this->count === $this->capacity) {
            throw new OverflowException('capacity:' . $this->capacity);
        }
        return 4 * $this->count++;
    }
    public function next(): int { return $this->reserve(); }
    public function count(): int { return $this->count; }
}
function drive(GuardCounter $counter, int $n): void {
    for ($i = 0; $i < $n; $i++) {
        try {
            printf("%s:%d\n", 'item' . $i, $counter->next());
        } catch (OverflowException $error) {
            echo $error->getMessage(), ':', $counter->count(), "\n";
        }
    }
}
drive(new GuardCounter(), $argc + 4);
"#;
    assert_both_modes(source, true,
        "item0:0\nitem1:4\nitem2:8\ncapacity:3:3\ncapacity:3:3\n");
}
