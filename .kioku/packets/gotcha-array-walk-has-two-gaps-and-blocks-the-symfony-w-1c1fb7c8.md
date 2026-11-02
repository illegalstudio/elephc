---
id: gotcha-array-walk-has-two-gaps-and-blocks-the-symfony-w-1c1fb7c8
type: gotcha
title: "array_walk has two gaps and blocks the Symfony --web binary"
description: "array_walk over a declared array parameter dies in the backend with PHP type Mixed, and a by-reference callback parameter is refused outright at top level; sort and usort on the same parameter work, so the by-ref boxing is not the cause"
created: 2026-09-17
sources:
  - path: src/codegen/lower_inst/builtins/arrays/reduce_sets.rs
    blob: 706f65e1830232773e7932562144c76cf7cf1964
  - path: src/ir_lower/expr/array_builtin_args.rs
    blob: 92e3e576411cf69a0c461522d64fd1464305074c
  - path: crates/elephc-builtin-contract/src/catalog_data.rs
    blob: 4a7f8a4e3fedc119cbdc01452030ffd89b5ec29c
superseded_by: "gotcha-by-reference-callback-parameters-need-the-descri-0c5cb472"
superseded_why: "Half of it is fixed (the element gate) and the remaining half has a precise root cause: the descriptor invoker passes arguments by value"
---

# array_walk has two gaps and blocks the Symfony --web binary

## Fact

`public/index.php --web` now TYPE-CHECKS with zero errors and stops in the backend:

    unsupported EIR backend feature: array_walk PHP type Mixed
    (Symfony\Component\Yaml\Command\LintCommand::displayJson)

PRE-EXISTING, not a regression: the clean `HEAD` compiler fails the same reduction. `HEAD` only
reached the linker because its smaller world never compiled `LintCommand` at all.

Two separate gaps, both reduced:

1. The array operand is `Mixed` at codegen even when the parameter is DECLARED `array`:

       function c(array $nums): string {
           array_walk($nums, static function (&$v) { $v *= 2; });
           return implode(',', $nums);
       }

   `eight_byte_callback_array_element_type` needs `PhpType::Array(elem)` and gets `Mixed`.
   NOT the by-reference contract: `sort($nums)` and `usort($nums, fn ($a, $b) => $b <=> $a)` on
   the same declared parameter both compile and run, and their contracts carry the identical
   `TypeSpec::Mixed, by_ref: true` first parameter. The difference is the argument-lowering path —
   the sorts have their own `BuiltinArgumentLowering` arms in `array_builtin_args.rs` while
   `array_walk` falls to the default `lower_args_with_signature`.

2. A by-reference callback parameter is refused outright at top level:

       $nums = [1, 2, 3];
       array_walk($nums, static function (&$v) { $v *= 2; });
       → array_walk() callback parameter $v must be passed a variable

   which is the entire point of `array_walk`.

The contract is also incomplete on its own terms: PHP declares
`array_walk(array|object &$array, callable $callback, mixed $arg = null): true` and the catalog
has two `TypeSpec::Mixed` parameters and no `$arg`. Changing it needs the docs pipeline, which
cannot run on this machine (see the docs-regeneration memory).

## Why

It is the only thing between a zero-error type-check and a linked Symfony --web binary
