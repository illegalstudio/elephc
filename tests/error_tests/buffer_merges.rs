//! Purpose:
//! Diagnostic regression tests for ternary, `?:`, `match` and `??` merges that put a `buffer<T>`
//! arm beside an arm of another type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A buffer has no null or `mixed` form: such a merge used to box the handle as an integer, so
//!   an element read through it answered null and a write through it was lost (task 7c3f5df2).
//!   The checker now rejects it at the merge, naming both types.

use super::*;

/// A ternary between two buffer element types is rejected with both types named.
#[test]
fn test_error_ternary_of_different_buffer_types() {
    expect_error(
        "<?php $f = buffer_new<float>(1); $i = buffer_new<int>(1); $x = $argc > 0 ? $f : $i;",
        "every arm of a ternary must have the same buffer type: buffer<float> cannot be merged with buffer<int>",
    );
}

/// A ternary with a `null` arm is rejected where it is indexed: a buffer is not nullable.
#[test]
fn test_error_ternary_of_buffer_and_null() {
    expect_error(
        "<?php $f = buffer_new<float>(1); echo ($argc > 0 ? $f : null)[0];",
        "buffer<float> cannot be merged with null",
    );
}

/// A `match` arm of a scalar type beside a buffer arm is rejected.
#[test]
fn test_error_match_of_buffer_and_scalar() {
    expect_error(
        "<?php $f = buffer_new<float>(1); $x = match ($argc) { 1 => $f, default => 0 };",
        "every arm of a match must have the same buffer type: buffer<float> cannot be merged with int",
    );
}

/// `?:` between two buffer element types is rejected like a ternary.
#[test]
fn test_error_short_ternary_of_different_buffer_types() {
    expect_error(
        "<?php $f = buffer_new<float>(1); $i = buffer_new<int>(1); $x = $f ?: $i;",
        "every arm of a `?:` must have the same buffer type: buffer<float> cannot be merged with buffer<int>",
    );
}

/// `??` between two buffer element types is rejected, as an assignment and where it is indexed.
#[test]
fn test_error_coalesce_of_different_buffer_types() {
    expect_error(
        "<?php $f = buffer_new<float>(1); $i = buffer_new<int>(1); $x = $f ?? $i;",
        "every arm of a `??` must have the same buffer type: buffer<float> cannot be merged with buffer<int>",
    );
    expect_error(
        "<?php $f = buffer_new<float>(1); $i = buffer_new<int>(1); echo ($f ?? $i)[0];",
        "every arm of a `??` must have the same buffer type: buffer<float> cannot be merged with buffer<int>",
    );
}

/// Buffers of two packed classes chosen in a method are rejected.
#[test]
fn test_error_ternary_of_different_packed_buffers_in_a_method() {
    expect_error(
        r#"<?php
packed class A { public float $x; }
packed class B { public float $x; }
final class Pair {
    public buffer<A> $a;
    public buffer<B> $b;
    public function __construct() {
        $this->a = buffer_new<A>(1);
        $this->b = buffer_new<B>(1);
    }
    public function pick(bool $first): float {
        return ($first ? $this->a : $this->b)[0]->x;
    }
}
"#,
        "every arm of a ternary must have the same buffer type",
    );
}
