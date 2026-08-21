---
title: "hexdec() — internals"
description: "Compiler internals for hexdec(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 306
---

## `hexdec()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/hexdec.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/hexdec.rs)
- **Lowering**: [`src/builtins/semantics.rs`:551](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L551) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.hexdec` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.hexdec`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function hexdec(string $hex_string): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `hexdec()`](../../../php/builtins/math/hexdec.md)
