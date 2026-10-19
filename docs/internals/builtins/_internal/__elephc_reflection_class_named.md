---
title: "__elephc_reflection_class_named() - internals"
description: "Compiler internals for __elephc_reflection_class_named(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1185
---

## `__elephc_reflection_class_named()` - internals

## Where it lives

- **Signature**: [`src/builtins/callables/__elephc_reflection_class_named.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/__elephc_reflection_class_named.rs)
- **Lowering**: [`src/builtins/semantics.rs`:700](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L700) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (2 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function __elephc_reflection_class_named(string $class_name): object
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference - this is a compiler internal helper._
