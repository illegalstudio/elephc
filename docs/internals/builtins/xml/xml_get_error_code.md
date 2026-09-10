---
title: "xml_get_error_code() - internals"
description: "Compiler internals for xml_get_error_code(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 940
---

## `xml_get_error_code()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1502](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1502) (`xml_get_error_code`)
- **Function symbol**: `xml_get_error_code()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_get_error_code(mixed $parser): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_get_error_code.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_get_error_code.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_get_error_code()`](../../../php/builtins/xml/xml_get_error_code.md)
