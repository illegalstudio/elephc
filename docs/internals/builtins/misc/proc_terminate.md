---
title: "proc_terminate() - internals"
description: "Compiler internals for proc_terminate(): lowering path, type checks, and runtime helpers."
---

## `proc_terminate()` - internals

## Where it lives

- **Signature**: [`src/builtins/proc/proc_terminate.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/proc/proc_terminate.rs)
- **Lowering**: [`src/builtins/semantics.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `checker_hook`
- **Result type source**: `shared`
- **Result ownership**: `non_heap`
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function proc_terminate(mixed $process, int $signal = 15): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `proc_terminate()`](../../../php/builtins/misc/proc_terminate.md)
