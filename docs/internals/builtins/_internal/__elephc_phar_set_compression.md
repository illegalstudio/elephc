---
title: "__elephc_phar_set_compression() — internals"
description: "Compiler internals for __elephc_phar_set_compression(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 469
---

## `__elephc_phar_set_compression()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_set_compression.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_set_compression.rs)
- **Lowering**: [`src/builtins/semantics.rs`:423](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L423) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Internal helper used by the built-in Phar / PharData support to change archive compression.
- Calls the native PHAR compression-control bridge and returns whether the update succeeded.
- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_phar_set_compression` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_phar_set_compression`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_phar_set_compression(string $filename, int $compression): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
