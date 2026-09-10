---
title: "xmlwriter_start_document() - internals"
description: "Compiler internals for xmlwriter_start_document(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1015
---

## `xmlwriter_start_document()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1264](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1264) (`xmlwriter_start_document`)
- **Function symbol**: `xmlwriter_start_document()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_start_document(mixed $writer, ?string $version = '1.0', ?string $encoding = null, ?string $standalone = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_document.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_document.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_start_document()`](../../../php/builtins/xml/xmlwriter_start_document.md)
