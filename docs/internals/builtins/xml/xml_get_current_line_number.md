---
title: "xml_get_current_line_number() - internals"
description: "Compiler internals for xml_get_current_line_number(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 939
---

## `xml_get_current_line_number()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1514](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1514) (`xml_get_current_line_number`)
- **Function symbol**: `xml_get_current_line_number()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_get_current_line_number(mixed $parser): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_get_current_line_number.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_get_current_line_number.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_get_current_line_number()`](../../../php/builtins/xml/xml_get_current_line_number.md)
