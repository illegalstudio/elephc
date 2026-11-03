---
title: "__elephc_last_error_str() - internals"
description: "Compiler internals for __elephc_last_error_str(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1287
---

## `__elephc_last_error_str()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_last_error_str.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_last_error_str.rs)
- **Lowering**: [`src/builtins/semantics.rs`:752](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L752) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Internal reader of the native last-diagnostic record, used by error_get_last().
- Field 0 answers the message and 1 the file, each as a fresh owned string.
- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_last_error_str` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `fresh`
- **Effects**: `static (3 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_last_error_str`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_last_error_str(int $field): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference - this is a compiler internal helper._
