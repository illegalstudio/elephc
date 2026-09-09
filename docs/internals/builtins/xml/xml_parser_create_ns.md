---
title: "xml_parser_create_ns() - internals"
description: "Compiler internals for xml_parser_create_ns(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 944
---

## `xml_parser_create_ns()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1348](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1348) (`xml_parser_create_ns`)
- **Function symbol**: `xml_parser_create_ns()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_parser_create_ns(?string $encoding = null, string $separator = ':'): mixed
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_create_ns.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_create_ns.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_parser_create_ns()`](../../../php/builtins/xml/xml_parser_create_ns.md)
