---
title: "join() — internals"
description: "Compiler internals for join(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 437
---

## `join()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/join.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/join.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.implode` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `independent`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.implode`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function join(mixed $separator, mixed $array = null): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `join()`](../../../php/builtins/string/join.md)
