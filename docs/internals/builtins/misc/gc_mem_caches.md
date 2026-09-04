---
title: "gc_mem_caches() - internals"
description: "Compiler internals for gc_mem_caches(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 333
---

## `gc_mem_caches()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/gc_mem_caches.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/gc_mem_caches.rs)
- **Lowering**: [`src/builtins/semantics.rs`:583](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L583) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (2 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function gc_mem_caches(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/gc_mem_caches.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/gc_mem_caches.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `gc_mem_caches()`](../../../php/builtins/misc/gc_mem_caches.md)
