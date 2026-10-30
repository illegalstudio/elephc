---
title: "__elephc_call_strict_types() - internals"
description: "Compiler internals for __elephc_call_strict_types(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1113
---

## `__elephc_call_strict_types()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_call_strict_types.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_call_strict_types.rs)
- **Lowering**: [`src/builtins/semantics.rs`:706](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L706) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (1 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function __elephc_call_strict_types(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference - this is a compiler internal helper._
