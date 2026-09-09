---
title: "xmlwriter_write_element() — internals"
description: "Compiler internals for xmlwriter_write_element(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 971
---

## `xmlwriter_write_element()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1123](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1123) (`xmlwriter_write_element`)
- **Function symbol**: `xmlwriter_write_element()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_write_element(mixed $writer, string $name, ?string $content = null): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_element.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_element.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_write_element()`](../../../php/builtins/xml/xmlwriter_write_element.md)
