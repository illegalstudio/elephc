---
title: "xml_get_current_byte_index() — internals"
description: "Compiler internals for xml_get_current_byte_index(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 913
---

## `xml_get_current_byte_index()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1573](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1573) (`xml_get_current_byte_index`)
- **Function symbol**: `xml_get_current_byte_index()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_get_current_byte_index(mixed $parser): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_get_current_byte_index.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_get_current_byte_index.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_get_current_byte_index()`](../../../php/builtins/xml/xml_get_current_byte_index.md)
