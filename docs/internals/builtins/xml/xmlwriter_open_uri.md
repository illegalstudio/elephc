---
title: "xmlwriter_open_uri() - internals"
description: "Compiler internals for xmlwriter_open_uri(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1007
---

## `xmlwriter_open_uri()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:951](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L951) (`xmlwriter_open_uri`)
- **Function symbol**: `xmlwriter_open_uri()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_open_uri(string $uri): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_open_uri.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_open_uri.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_open_uri()`](../../../php/builtins/xml/xmlwriter_open_uri.md)
