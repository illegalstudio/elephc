---
id: bug_fix-array-push-takes-at-least-one-argument-and-retur-3cda37e7
type: bug_fix
title: "array_push takes at least one argument and returns the new element count"
description: "The contract said min 2 and Void, so array_push($a) was refused and $n = array_push(...) was empty when compiled while the interpreter answered the count"
created: 2026-09-17
sources:
  - path: crates/elephc-builtin-contract/src/catalog_data.rs
    blob: 4a7f8a4e3fedc119cbdc01452030ffd89b5ec29c
  - path: src/builtins/array/array_push.rs
    blob: 1bcd6580a5ba04e38292dc62ba6bd2eae95403d2
  - path: src/codegen/lower_inst/builtins/arrays/basic.rs
    blob: 1066583379bcb5838e39709a00bfb7763caac72d
  - path: src/ir_lower/expr/array_builtin_args.rs
    blob: a4a267a3f810c7469cabb8b61b1bc9cd4e642695
---

# array_push takes at least one argument and returns the new element count

## Fact

`array_push()` takes at least ONE argument and answers the array's new element count.

Two deviations were carried in the contract on purpose ("reproducing the legacy behavior exactly"),
and both were observable against `php -n` 8.5:

* `min_args: 2` refused `array_push($a)`, which PHP accepts — it appends nothing and returns
  `count($a)`. PHP's own diagnostic is `expects at least 1 argument`.
* `returns: Void` threw the result away, so `$n = array_push($a, 3, 4);` printed nothing compiled
  and `4` interpreted — the eval interpreter has answered the count all along
  (`eval_array_push_unshift_count_result`), so AOT and eval disagreed on the same source.

The count has to be produced in TWO places, which is what made the first fix look complete when it
was not:

1. `lower_array_push` (the runtime-call path, two or more values) reads it after the appends: growth
   can move the container and the by-ref write-back has already published the current pointer. The
   receiver shape is the one the appends used — `__rt_mixed_count` for a boxed cell, the header
   length word otherwise.
2. `lower_static_array_push` (`src/ir_lower/expr/array_builtin_args.rs`) is the fast path for
   `array_push($local, $value)` over an indexed local — exactly ONE value — and it skips the runtime
   call to emit the indexed append directly. It returned `null`. `$n = array_push($big, $i)` inside
   a loop therefore printed nothing while the two-value form next to it printed the count. It now
   reloads the local after the write-back and emits `Op::ArrayLen`.

Fixture that covers both plus the boxed and property receivers, all matching `php -n`:

```php
function boxed($a) { return array_push($a, 'x', 'y'); }      // 4
function boxedNone($a) { return array_push($a); }            // 2
echo array_push($h->rows, 'b');                              // 2
for ($i = 0; $i < 40; $i++) { $n = array_push($big, $i); }   // 40
```

`tests/error_tests/array_builtins.rs::test_error_array_push_wrong_args` now expects
`array_push() takes at least 1 argument`.
