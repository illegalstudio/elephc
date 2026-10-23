---
title: "__elephc_bcnumber_divmod() - internals"
description: "Compiler internals for __elephc_bcnumber_divmod(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1081
---

## `__elephc_bcnumber_divmod()` - internals

## Where it lives

- **Signature**: [`src/builtins/math/__elephc_bcnumber_divmod.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/__elephc_bcnumber_divmod.rs)
- **Lowering**: [`src/builtins/semantics.rs`:704](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L704) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_bcnumber_divmod` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (3 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_bcnumber_divmod`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_bcnumber_divmod(string $num1, string $num2, ?int $scale = null): array
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference - this is a compiler internal helper._
