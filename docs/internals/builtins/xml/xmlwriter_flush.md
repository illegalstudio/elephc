---
title: "xmlwriter_flush() - internals"
description: "Compiler internals for xmlwriter_flush(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 968
---

## `xmlwriter_flush()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1466](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1466) (`xmlwriter_flush`)
- **Function symbol**: `xmlwriter_flush()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_flush(mixed $writer, bool $empty = true): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_flush.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_flush()`](../../../php/builtins/xml/xmlwriter_flush.md)
