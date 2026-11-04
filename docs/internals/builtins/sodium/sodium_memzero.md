---
title: "sodium_memzero() - internals"
description: "Compiler internals for sodium_memzero(): lowering path, type checks, and runtime helpers."
---

## `sodium_memzero()` - internals

## Where it lives

- **Signature**: [`src/builtins/sodium/bindings.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/sodium/bindings.rs)
- **Lowering**: [`src/builtins/semantics.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `shared`
- **Result type source**: `shared`
- **Result ownership**: `fresh`
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function sodium_memzero(string $string): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$string`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `sodium_memzero()`](../../../php/builtins/sodium/sodium_memzero.md)
