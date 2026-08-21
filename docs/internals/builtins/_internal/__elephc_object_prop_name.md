---
title: "__elephc_object_prop_name() — internals"
description: "Compiler internals for __elephc_object_prop_name(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 530
---

## `__elephc_object_prop_name()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/__elephc_object_prop_name.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/__elephc_object_prop_name.rs)
- **Lowering**: [`src/builtins/semantics.rs`:544](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L544) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_object_prop_name` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (1 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_object_prop_name`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_object_prop_name(mixed $value, int $index): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
