//! Purpose:
//! Runs php-src 8.6's `Time\Duration` serialization fixture.
//!
//! Called from:
//! - `cargo test --test codegen_tests time_duration_serialization_php86`.
//!
//! Key details:
//! - Transcribed from `ext/date/tests/time/duration/serialize.phpt`.
//! - `helper.inc`'s `f()` is inlined and accepts `mixed` with an `instanceof` guard because
//!   `unserialize()` returns `mixed`; the guard is behavior-preserving and lets the static
//!   checker narrow without changing stdout.
//! - The manual `__unserialize()` call on an initialized duration must raise the same
//!   `Invalid serialization data` exception php-src raises.

use crate::support::*;

/// Round-trips a duration and rejects every malformed serialized payload.
#[test]
fn test_time_duration_serialization_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

function f(mixed $d, bool $pad = true, bool $plus_sign = true): string {
    if (!$d instanceof Time\Duration) {
        throw new \TypeError("f(): Argument #1 ($d) must be of type Time\\Duration");
    }
    $result = sprintf("%s%d.%09d", ($d->negative ? "-" : ($plus_sign ? "+" : " ")), $d->seconds, $d->nanoseconds);
    if ($pad) {
        $result = str_pad($result, 21, " ", STR_PAD_LEFT);
    }
    return $result;
}

var_dump($serialized = serialize(Time\Duration::fromSeconds(1, 2)->negate()));
$unserialized = unserialize($serialized);
if (!$unserialized instanceof Time\Duration) {
    throw new \Error('unreachable');
}
echo f($unserialized), PHP_EOL;
var_dump(serialize($unserialized));
echo f($unserialized->add($unserialized)), PHP_EOL;

try {
    // $negative is not bool, but coercible.
    echo f(unserialize('O:13:"Time\Duration":3:{s:7:"seconds";i:1;s:11:"nanoseconds";i:1;s:8:"negative";i:999;}')), PHP_EOL;
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}

try {
    // $negative is not bool and not coercible.
    unserialize('O:13:"Time\Duration":3:{s:7:"seconds";i:1;s:11:"nanoseconds";i:1;s:8:"negative";N;}');
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}

try {
    // $seconds is negative.
    unserialize('O:13:"Time\Duration":3:{s:7:"seconds";i:-1;s:11:"nanoseconds";i:1;s:8:"negative";b:0;}');
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}

try {
    // Dynamic property.
    unserialize('O:13:"Time\Duration":4:{s:7:"seconds";i:1;s:11:"nanoseconds";i:1;s:8:"negative";b:0;s:3:"foo";N;}');
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}


try {
    // Out of range nanoseconds
    unserialize('O:13:"Time\Duration":3:{s:7:"seconds";i:1;s:11:"nanoseconds";i:1000000000;s:8:"negative";b:0;}');
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}

try {
    // Nanoseconds that would be truncated to a valid value by a 32-bit conversion (2^32 + 5).
    unserialize('O:13:"Time\Duration":3:{s:7:"seconds";i:1;s:11:"nanoseconds";i:4294967301;s:8:"negative";b:0;}');
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}

try {
    // A negative zero duration is rejected.
    unserialize('O:13:"Time\Duration":3:{s:7:"seconds";i:0;s:11:"nanoseconds";i:0;s:8:"negative";b:1;}');
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}

try {
    Time\Duration::fromSeconds(1, 1)
        ->__unserialize([
            'seconds' => 2,
            'nanoseconds' => 2,
            'negative' => true,
        ]);
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), PHP_EOL;
}

"#);
    assert_eq!(out.trim(), r#"string(85) "O:13:"Time\Duration":3:{s:7:"seconds";i:1;s:11:"nanoseconds";i:2;s:8:"negative";b:1;}"
         -1.000000002
string(85) "O:13:"Time\Duration":3:{s:7:"seconds";i:1;s:11:"nanoseconds";i:2;s:8:"negative";b:1;}"
         -2.000000004
Exception: Invalid serialization data for Time\Duration object
Exception: Invalid serialization data for Time\Duration object
Exception: Invalid serialization data for Time\Duration object
Exception: Invalid serialization data for Time\Duration object
Exception: Invalid serialization data for Time\Duration object
Exception: Invalid serialization data for Time\Duration object
Exception: Invalid serialization data for Time\Duration object
Exception: Invalid serialization data for Time\Duration object
"#.trim());
}
