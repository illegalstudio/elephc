---
title: "mt_rand() — internals"
description: "Compiler internals for mt_rand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 278
---

## `mt_rand()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/mt_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/mt_rand.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.mt_rand` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.mt_rand`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function mt_rand(int $min, int $max): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/mt_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/mt_rand.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `mt_rand()`](../../../php/builtins/math/mt_rand.md)
