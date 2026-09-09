---
title: "xml_parser_create() — internals"
description: "Compiler internals for xml_parser_create(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 918
---

## `xml_parser_create()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1328](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1328) (`xml_parser_create`)
- **Function symbol**: `xml_parser_create()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_parser_create(?string $encoding = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_create.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_create.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_parser_create()`](../../../php/builtins/xml/xml_parser_create.md)
