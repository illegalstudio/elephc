---
title: "spl_autoload_register() — internals"
description: "Compiler internals for spl_autoload_register(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 347
---

## `spl_autoload_register()` — internals

## Where it lives

- **Signature**: [`src/builtins/spl/spl_autoload_register.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/spl/spl_autoload_register.rs)
- **Lowering**: [`src/builtins/semantics.rs`:423](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L423) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.spl_autoload_register` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.spl_autoload_register`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function spl_autoload_register(callable $callback = null, bool $throw = true, bool $prepend = false): bool
```

## What the type checker enforces

- **Arity**: takes 0–3 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_register.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_register.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `spl_autoload_register()`](../../../php/builtins/spl/spl_autoload_register.md)
