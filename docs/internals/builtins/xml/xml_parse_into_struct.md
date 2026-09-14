---
title: "xml_parse_into_struct() - internals"
description: "Compiler internals for xml_parse_into_struct(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 978
---

## `xml_parse_into_struct()` - internals

## Where it lives

- **Signature**: [`src/builtins/xml/xml_parse_into_struct.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/xml/xml_parse_into_struct.rs)
- **Lowering**: [`src/builtins/semantics.rs`:643](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L643) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function xml_parse_into_struct(mixed $parser, string $data, mixed $values, mixed $index = null): int
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).
- **By-reference parameters**: `$values`, `$index`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_parse_into_struct.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parse_into_struct.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$values`, `$index`.

## Cross-references

- [User reference for `xml_parse_into_struct()`](../../../php/builtins/xml/xml_parse_into_struct.md)
