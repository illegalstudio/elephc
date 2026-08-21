---
title: "pcntl_waitid() — internals"
description: "Compiler internals for pcntl_waitid(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 350
---

## `pcntl_waitid()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_waitid.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_waitid.rs)
- **Lowering**: [`src/builtins/semantics.rs`:611](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L611) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.waitid` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (4 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.waitid`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_waitid(int $idtype = 0, int $id = null, mixed $info = [], int $flags = 4): bool
```

## What the type checker enforces

- **Arity**: takes 0–4 arguments (4 optional).
- **By-reference parameters**: `$info`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_waitid.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_waitid.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$info`.

## Cross-references

- [User reference for `pcntl_waitid()`](../../../php/builtins/misc/pcntl_waitid.md)
