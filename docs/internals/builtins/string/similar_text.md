---
title: "similar_text() — internals"
description: "Compiler internals for similar_text(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 513
---

## `similar_text()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/similar_text.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/similar_text.rs)
- **Lowering**: [`src/builtins/semantics.rs`:560](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L560) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `shared`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function similar_text(string $string1, string $string2, mixed $percent = null): int
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).
- **By-reference parameters**: `$percent`.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `similar_text()`](../../../php/builtins/string/similar_text.md)
