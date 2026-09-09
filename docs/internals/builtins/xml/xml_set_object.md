---
title: "xml_set_object() — internals"
description: "Compiler internals for xml_set_object(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 929
---

## `xml_set_object()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1360](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1360) (`xml_set_object`)
- **Function symbol**: `xml_set_object()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_set_object(mixed $parser, mixed $object): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_object.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_object.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_set_object()`](../../../php/builtins/xml/xml_set_object.md)
