---
title: "proc_open() - internals"
description: "Compiler internals for proc_open(): lowering path, type checks, and runtime helpers."
---

## `proc_open()` - internals

## Where it lives

- **Signature**: [`src/builtins/proc/proc_open.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/proc/proc_open.rs)
- **Lowering**: [`src/builtins/semantics.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `checker_hook`
- **Result type source**: `shared`
- **Result ownership**: `fresh`
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function proc_open(array|string $command, array $descriptor_spec, mixed $pipes, ?string $cwd = null, ?array $env_vars = null, ?array $options = null): mixed
```

## What the type checker enforces

- **Arity**: takes 3–6 arguments (3 optional).
- **By-reference parameters**: `$pipes`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `proc_open()`](../../../php/builtins/misc/proc_open.md)
