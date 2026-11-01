---
id: bug_fix-a-hash-slot-and-a-bare-array-contract-must-promi-3720a94c
type: bug_fix
title: "A hash slot and a bare array contract must promise a REPRESENTATION, not a PHP type - four silent miscompiles"
description: "One root shape: a container's static payload type said something the writer never stored, and every read of that slot then misread it"
created: 2026-09-18
verified_by: "Four reductions byte-diffed against php -n 8.5; error_tests 1431/99, codegen::regressions 417/6 (baseline 415/7), types 343/16, autoload 104/2, type_builtins 134/6, codegen::arrays 665/27 - all at or better than baseline"
sources:
  - path: src/ir_lower/stmt/array_write_core.rs
    blob: 25aa1921f359ce30ac026c64e196003dbf5b4d3a
  - path: src/types/checker/functions/returns.rs
    blob: f985c904c9ad1422d71a31863629be80b70549c1
  - path: tests/codegen/regressions/arrays.rs
    blob: 625cb82beba427a5e45e578ef9e0e6d5bbbcf87c
---

# A hash slot and a bare array contract must promise a REPRESENTATION, not a PHP type - four silent miscompiles

## Fact

An elephc container carries ONE payload representation: a raw pointer, a boxed Mixed cell, a
16-byte string pair. The static element/value type is what every READ materializes through, so it
is a promise about that representation -- not about what PHP values the slot may hold.

FOUR places broke that promise. All four are fixed, each with a regression test.

1. `assoc_array_write_updated_type` (`src/ir_lower/stmt/array_write_core.rs`) decided whether a
   hash needs widening by NORMALIZING both sides first, and the normalizer maps every refcounted
   non-string type to `Mixed`. An `array<string, N>` therefore looked like it already accepted
   anything:
       $nodes = ['node' => $node]; $nodes['n'] = 1;      // compiled, then SIGSEGV
       $nodes['arguments'] = $nullableNode;              // backend: hash_set value PHP type Mixed
   Twig's `TestExpression::__construct` writes the second shape. Now the question asked is whether
   the STORAGE can represent the written payload, and only an object-into-object slot is exempt
   (property and method access dispatch on the runtime class id).

2. The same function let two CONTAINER families share a slot, so `['x' => [1, 2]]` followed by
   `$a['y'] = ['s', 't']` printed the element's raw pointer as an integer. Containers now widen.

3. `generic_array_return_contract` (`src/types/checker/functions/returns.rs`) resolved a bare
   `array` hint to a CONTAINER payload. Symfony's `BufferingLogger::cleanLogs(): array` is the
   silent half -- `$this->logs[] = [$level, $message, $context]` boxes each row, the contract said
   `array<array<mixed>>`, and `$log[1]` read a boxed cell as a raw array: `count($log)` answered 4
   and the values came out as pointers. Twig's `Template::getBlocks(): array` is the loud half:
   its base default is `[]`, its subclasses assign a hash of arrays, and the backend refused
   `runtime_call from PHP type Array(Mixed) to PHP type AssocArray { .. }`. A bare `array` now
   promises the STORAGE KIND only; a scalar payload keeps its precision, a container payload
   becomes `Mixed`.

HOW TO FIND THE NEXT ONE. The symptom is always a read that materializes through a type the writer
did not store: a raw pointer printed as an int, a `count()` that is one or two too high, or a
backend `... value PHP type ...` refusal. `--emit-ir` and compare the type on the WRITE with the
type on the READ: `prop_get ... php=array<mixed>` against a call site's
`php=array<array<mixed>>` is the whole bug in two lines.

## Why

Three of the four produced a running binary that printed the wrong thing; the fourth stopped the Symfony --web build
