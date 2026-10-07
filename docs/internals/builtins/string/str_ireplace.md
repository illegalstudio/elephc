---
title: "str_ireplace() - internals"
description: "Compiler internals for str_ireplace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 913
---

## `str_ireplace()` - internals

## Where it lives

- **Signature**: [`src/builtins/string/str_ireplace.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/str_ireplace.rs)
- **Lowering**: [`src/builtins/semantics.rs`:695](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L695) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `checker_hook`
- **Result type source**: `shared`
- **Result ownership**: `independent`
- **Effects**: `shared`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function str_ireplace(array|string $search, array|string $replace, array|string $subject, int $count = null): array|string
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).
- **By-reference parameters**: `$count`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/str_ireplace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_ireplace.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$count`.

## Cross-references

- [User reference for `str_ireplace()`](../../../php/builtins/string/str_ireplace.md)
