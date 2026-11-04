---
title: "sodium_crypto_generichash_final() - internals"
description: "Compiler internals for sodium_crypto_generichash_final(): lowering path, type checks, and runtime helpers."
---

## `sodium_crypto_generichash_final()` - internals

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
function sodium_crypto_generichash_final(string $state, int $length = 32): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).
- **By-reference parameters**: `$state`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `sodium_crypto_generichash_final()`](../../../php/builtins/sodium/sodium_crypto_generichash_final.md)
