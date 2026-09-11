---
title: "xmlwriter_write_dtd_entity() — internals"
description: "Compiler internals for xmlwriter_write_dtd_entity(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 971
---

## `xmlwriter_write_dtd_entity()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1437](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1437) (`xmlwriter_write_dtd_entity`)
- **Function symbol**: `xmlwriter_write_dtd_entity()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_write_dtd_entity(mixed $writer, string $name, string $content, bool $isParam = false, ?string $publicId = null, ?string $systemId = null, ?string $notationData = null): bool
```

## What the type checker enforces

- **Arity**: takes 3–7 arguments (4 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_dtd_entity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_dtd_entity.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_write_dtd_entity()`](../../../php/builtins/xml/xmlwriter_write_dtd_entity.md)
