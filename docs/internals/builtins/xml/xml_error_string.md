---
title: "xml_error_string() — internals"
description: "Compiler internals for xml_error_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 912
---

## `xml_error_string()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1540](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1540) (`xml_error_string`)
- **Function symbol**: `xml_error_string()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_error_string(int $error_code): ?string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_error_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_error_string.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_error_string()`](../../../php/builtins/xml/xml_error_string.md)
