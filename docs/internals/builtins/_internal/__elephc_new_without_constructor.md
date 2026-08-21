---
title: "__elephc_new_without_constructor() — internals"
description: "Compiler internals for __elephc_new_without_constructor(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 529
---

## `__elephc_new_without_constructor()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_new_without_constructor.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_new_without_constructor.rs)
- **Lowering**: [`src/builtins/semantics.rs`:551](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L551) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `fresh`
- **Effects**: `static (3 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function __elephc_new_without_constructor(string $class): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
