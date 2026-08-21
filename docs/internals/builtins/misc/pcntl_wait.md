---
title: "pcntl_wait() — internals"
description: "Compiler internals for pcntl_wait(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 349
---

## `pcntl_wait()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_wait.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_wait.rs)
- **Lowering**: [`src/builtins/semantics.rs`:611](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L611) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.wait` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (4 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.wait`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_wait(mixed $status, int $flags = 0, mixed $resource_usage = []): int
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).
- **By-reference parameters**: `$status`, `$resource_usage`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wait.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wait.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$status`, `$resource_usage`.

## Cross-references

- [User reference for `pcntl_wait()`](../../../php/builtins/misc/pcntl_wait.md)
