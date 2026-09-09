---
title: "xml_set_default_handler() - internals"
description: "Compiler internals for xml_set_default_handler(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 949
---

## `xml_set_default_handler()` - internals

## Where it lives

- **Signature**: [`src/builtins/xml/xml_set_default_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/xml/xml_set_default_handler.rs)
- **Lowering**: [`src/builtins/semantics.rs`:674](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L674) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (6 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function xml_set_default_handler(mixed $parser, mixed $handler): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_default_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_default_handler.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_set_default_handler()`](../../../php/builtins/xml/xml_set_default_handler.md)
