---
title: "__elephc_curl_share_init_persistent() — internals"
description: "Compiler internals for __elephc_curl_share_init_persistent(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 580
---

## `__elephc_curl_share_init_persistent()` — internals

## Where it lives

- **Signature**: [`src/builtins/curl/__elephc_curl_share_init_persistent.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/curl/__elephc_curl_share_init_persistent.rs)
- **Lowering**: [`src/builtins/semantics.rs`:544](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L544) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_curl_share_init_persistent` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `fresh`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_curl_share_init_persistent`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_curl_share_init_persistent(string $lock_data_csv): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
