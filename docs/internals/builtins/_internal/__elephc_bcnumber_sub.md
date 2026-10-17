---
title: "__elephc_bcnumber_sub() - internals"
description: "Compiler internals for __elephc_bcnumber_sub(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1082
---

## `__elephc_bcnumber_sub()` - internals

## Where it lives

- **Signature**: [`src/builtins/math/__elephc_bcnumber_sub.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/__elephc_bcnumber_sub.rs)
- **Lowering**: [`src/builtins/semantics.rs`:700](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L700) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_bcnumber_sub` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `fresh`
- **Effects**: `static (3 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_bcnumber_sub`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_bcnumber_sub(string $num1, string $num2, ?int $scale = null): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference - this is a compiler internal helper._
