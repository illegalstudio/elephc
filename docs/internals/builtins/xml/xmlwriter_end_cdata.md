---
title: "xmlwriter_end_cdata() — internals"
description: "Compiler internals for xmlwriter_end_cdata(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 934
---

## `xmlwriter_end_cdata()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1217](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1217) (`xmlwriter_end_cdata`)
- **Function symbol**: `xmlwriter_end_cdata()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_end_cdata(mixed $writer): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_cdata.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_cdata.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_end_cdata()`](../../../php/builtins/xml/xmlwriter_end_cdata.md)
