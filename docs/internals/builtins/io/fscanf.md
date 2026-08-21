---
title: "fscanf() — internals"
description: "Compiler internals for fscanf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 186
---

## `fscanf()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fscanf.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fscanf.rs)
- **Lowering**: [`src/builtins/semantics.rs`:551](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L551) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `shared`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function fscanf(resource $stream, string $format, ...$vars): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **Variadic**: collects excess arguments into `$vars`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fscanf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fscanf.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$vars`.

## Cross-references

- [User reference for `fscanf()`](../../../php/builtins/io/fscanf.md)
