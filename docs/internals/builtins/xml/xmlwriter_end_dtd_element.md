---
title: "xmlwriter_end_dtd_element() — internals"
description: "Compiler internals for xmlwriter_end_dtd_element(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 940
---

## `xmlwriter_end_dtd_element()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1353](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1353) (`xmlwriter_end_dtd_element`)
- **Function symbol**: `xmlwriter_end_dtd_element()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_end_dtd_element(mixed $writer): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd_element.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd_element.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_end_dtd_element()`](../../../php/builtins/xml/xmlwriter_end_dtd_element.md)
