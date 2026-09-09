---
title: "xml_parser_free() — internals"
description: "Compiler internals for xml_parser_free(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 920
---

## `xml_parser_free()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1584](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1584) (`xml_parser_free`)
- **Function symbol**: `xml_parser_free()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_parser_free(mixed $parser): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_free.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_free.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_parser_free()`](../../../php/builtins/xml/xml_parser_free.md)
