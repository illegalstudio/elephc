---
title: "__elephc_strtotime_raw() — internals"
description: "Compiler internals for __elephc_strtotime_raw(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 486
---

## `__elephc_strtotime_raw()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_strtotime_raw.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_strtotime_raw.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Internal helper used by the strtotime() builtin.
- Provides a raw timestamp parsing path for the runtime strtotime helper.
- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_strtotime_raw` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `shared`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_strtotime_raw`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_strtotime_raw(string $datetime, int $baseTimestamp = null): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
