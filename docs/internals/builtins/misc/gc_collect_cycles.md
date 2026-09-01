---
title: "gc_collect_cycles() — internals"
description: "Compiler internals for gc_collect_cycles(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 326
---

## `gc_collect_cycles()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/gc_collect_cycles.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/gc_collect_cycles.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `shared`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function gc_collect_cycles(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gc_collect_cycles()`](../../../php/builtins/misc/gc_collect_cycles.md)
